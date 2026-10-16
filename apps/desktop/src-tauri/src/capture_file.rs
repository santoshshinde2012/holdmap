//! Fixed-name, bounded reads from native-owned private capture directories.
#[cfg(unix)]
use std::io::Read;
use std::path::Path;

#[cfg(unix)]
pub(crate) fn read_private(
    directory: &Path,
    filename: &'static str,
    expected: u64,
    limit: u64,
) -> Result<Vec<u8>, String> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if expected == 0 || expected > limit {
        return Err("This recording format is unavailable or too large.".into());
    }
    let parent = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(directory)
        .map_err(|_| "Couldn't verify the private recording directory.".to_owned())?;
    let parent_meta = parent
        .metadata()
        .map_err(|_| "Couldn't verify the recording directory.".to_owned())?;
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    if parent_meta.uid() != me || parent_meta.mode() & 0o077 != 0 {
        return Err("The recording directory is not private.".into());
    }
    let name = std::ffi::CString::new(filename).expect("fixed recording filename");
    // SAFETY: parent owns the directory descriptor; the fixed filename is NUL-terminated.
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err("Couldn't open the recording safely.".into());
    }
    // SAFETY: openat returned this newly owned descriptor.
    let file = unsafe { std::fs::File::from_raw_fd(descriptor) };
    let metadata = file
        .metadata()
        .map_err(|_| "Couldn't verify the recording file.".to_owned())?;
    if !metadata.is_file()
        || metadata.uid() != me
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
        || metadata.len() != expected
    {
        return Err("The recording file is not a private, complete regular file.".into());
    }
    let mut bytes = Vec::with_capacity(expected as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Couldn't read the recording.".to_owned())?;
    if bytes.len() as u64 != expected {
        return Err("The recording data is incomplete or invalid.".into());
    }
    Ok(bytes)
}

#[cfg(not(unix))]
pub(crate) fn read_private(_: &Path, _: &'static str, _: u64, _: u64) -> Result<Vec<u8>, String> {
    Err("Native recording files are unavailable on this platform.".into())
}
