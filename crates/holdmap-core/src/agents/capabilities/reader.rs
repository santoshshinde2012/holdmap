//! Open only the selected regular file, refusing path redirection before reading its bytes.

use std::fs::File;
use std::io;
use std::path::Path;

#[cfg(unix)]
pub(super) fn open(path: &Path) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Component;

    let components: Vec<_> = path.components().collect();
    if components.first() != Some(&Component::RootDir)
        || components.len() < 2
        || components[1..]
            .iter()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut parent = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open("/")?;
    for (index, component) in components[1..].iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(io::ErrorKind::InvalidInput.into());
        };
        let name =
            std::ffi::CString::new(name.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput)?;
        let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC;
        if index + 2 < components.len() {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: parent owns a directory descriptor; name is NUL-terminated and flags
        // open only this component. Each next directory stays anchored to its held parent.
        let descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: openat returned a newly owned descriptor, closed by File on every path.
        parent = unsafe { File::from_raw_fd(descriptor) };
    }
    Ok(parent)
}

#[cfg(windows)]
pub(super) fn open(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::{Component, Prefix};
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;

    match path.components().next() {
        Some(Component::Prefix(prefix))
            if matches!(
                prefix.kind(),
                Prefix::Disk(_)
                    | Prefix::VerbatimDisk(_)
                    | Prefix::UNC(..)
                    | Prefix::VerbatimUNC(..)
            ) => {}
        _ => return Err(io::ErrorKind::InvalidInput.into()),
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    verify(&file, path)?;
    Ok(file)
}

/// Windows has no public root-relative CreateFile API. Verify the opened handle's fully
/// resolved path before reading, so a raced parent junction cannot redirect file contents.
#[cfg(windows)]
pub(super) fn verify(file: &File, path: &Path) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;

    let metadata = file.metadata()?;
    if super::is_link(&metadata) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut buffer = vec![0u16; 32768];
    // SAFETY: file retains the valid handle, and buffer has the declared writable length.
    // Flags zero request the fully normalized DOS path, including resolved parent links.
    let size = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle().cast(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if size == 0 || size as usize >= buffer.len() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let actual =
        String::from_utf16(&buffer[..size as usize]).map_err(|_| io::ErrorKind::InvalidInput)?;
    let expected = path.to_str().ok_or(io::ErrorKind::InvalidInput)?;
    if !normalized(&actual).eq_ignore_ascii_case(&normalized(expected)) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

#[cfg(windows)]
fn normalized(path: &str) -> String {
    let path = path.replace('/', "\\");
    if path
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("\\\\?\\UNC\\"))
    {
        format!("\\\\{}", &path[8..])
    } else {
        path.strip_prefix("\\\\?\\").unwrap_or(&path).into()
    }
}

#[cfg(not(windows))]
pub(super) fn verify(_: &File, _: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(super) fn open(_: &Path) -> io::Result<File> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[cfg(unix)]
    #[test]
    fn replacement_of_a_checked_parent_by_a_symlink_cannot_redirect_the_reader() {
        use std::os::unix::fs::symlink;
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let parent = root.join("selected");
        let outside = root.join("outside");
        std::fs::create_dir(&parent).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let selected = parent.join("config.json");
        std::fs::write(&selected, b"selected fixture").unwrap();
        std::fs::write(outside.join("config.json"), b"private-outside-fixture").unwrap();
        assert!(!std::fs::symlink_metadata(&parent)
            .unwrap()
            .file_type()
            .is_symlink());
        // Model the exact gap after path prechecks and before the production reader opens.
        std::fs::rename(&parent, root.join("original")).unwrap();
        symlink(&outside, &parent).unwrap();
        assert!(
            open(&selected).is_err(),
            "parent redirection must be rejected before reading contents"
        );
        std::fs::remove_file(&parent).unwrap();
        std::fs::rename(root.join("original"), &parent).unwrap();
        let mut bytes = Vec::new();
        open(&selected).unwrap().read_to_end(&mut bytes).unwrap();
        assert!(
            bytes == b"selected fixture",
            "only the selected fixture may be read"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_handle_identity_matches_only_its_selected_file_and_normalizes_dos_prefixes() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let path = root.join("config.json");
        let other = root.join("other.json");
        std::fs::write(&path, b"selected fixture").unwrap();
        std::fs::write(&other, b"private-other-fixture").unwrap();
        let mut file = open(&path).unwrap();
        assert!(verify(&file, &path).is_ok());
        assert!(verify(&file, &other).is_err());
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert!(
            bytes == b"selected fixture",
            "only the selected fixture may be read"
        );
        assert_eq!(
            normalized("\\\\?\\C:\\Users\\test\\config.json"),
            "C:\\Users\\test\\config.json"
        );
        assert_eq!(
            normalized("\\\\?\\UNC\\host\\share\\config.json"),
            "\\\\host\\share\\config.json"
        );
    }
}
