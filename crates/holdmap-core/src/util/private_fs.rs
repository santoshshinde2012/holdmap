//! Owner-only files and directories.

#[allow(unused_imports)]
use super::*;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::process::{Command, Stdio};
#[allow(unused_imports)]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Create (or truncate) a regular single-link file without following a final link. Unix
/// checks ownership and enforces 0600; Windows inherits the parent directory's ACL.
pub fn create_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        o.mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let f = o.open(path)?;
        check_owned_file(&f)?;
        // `mode` only applies to new files; tighten one left over from an older version.
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.set_len(0)?;
        Ok(f)
    }
    #[cfg(not(unix))]
    {
        if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private files must be regular files, not links",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let f = o.open(path)?;
        #[cfg(windows)]
        check_windows_private_file(&f)?;
        if !f.metadata()?.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private files must be regular files",
            ));
        }
        f.set_len(0)?;
        Ok(f)
    }
}

#[cfg(windows)]
pub(crate) fn check_windows_private_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT,
    };
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: File owns a live handle, and info points to writable storage of the required size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: the successful call initialized info.
    let info = unsafe { info.assume_init() };
    if info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private files must be regular files without reparse points or extra links",
        ));
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn check_owned_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    // SAFETY: geteuid has no preconditions.
    let me = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.uid() != me || metadata.nlink() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private files must be owned regular files with no extra links",
        ));
    }
    Ok(())
}

/// Open an existing owner-controlled state file without following its final symlink.
/// Unix files writable by other accounts or linked to another name are refused.
#[cfg(not(unix))]
pub(crate) fn open_private_read(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "state files must be regular files, not links",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        let file = options.open(path)?;
        #[cfg(windows)]
        check_windows_private_file(&file)?;
        Ok(file)
    }
}

/// Create a directory (and parents) that only its owner can enter (0700 on Unix for the
/// directories this creates; an existing owned directory is tightened too). Links and
/// directories owned by another Unix account are refused.
pub fn create_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)?;
    let metadata = std::fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private state directories must not be links",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private state directories must not be reparse points",
            ));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(dir)?;
        // SAFETY: geteuid has no preconditions.
        if file.metadata()?.uid() != unsafe { libc::geteuid() } {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "private state directory belongs to another account",
            ));
        }
        file.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
