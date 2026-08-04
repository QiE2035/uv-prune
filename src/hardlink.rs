use std::io;
use std::path::Path;

/// Trait to check whether a filesystem entry is a hard link
/// (i.e., has more than one directory entry pointing to the same inode).
pub trait IsHardLink {
    /// Returns `Ok(true)` if the entry has multiple hard links,
    /// `Ok(false)` if it has exactly one, or an `Err` on I/O failure.
    fn is_hardlink(&self) -> io::Result<bool>;
}

// ---------------------------------------------------------------------------
// Windows implementation — uses GetFileInformationByHandle to read nNumberOfLinks
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
impl<P: AsRef<Path>> IsHardLink for P {
    fn is_hardlink(&self) -> io::Result<bool> {
        use std::fs::File;
        use std::os::windows::io::AsRawHandle;

        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };

        let file = File::open(self)?;
        let handle = HANDLE(file.as_raw_handle());

        // SAFETY: BY_HANDLE_FILE_INFORMATION is a POD struct; zeroed is fine.
        let mut file_info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };

        // SAFETY: `handle` is a valid handle obtained from `File::as_raw_handle()`,
        // and `file_info` is a valid output buffer.
        let result = unsafe { GetFileInformationByHandle(handle, &mut file_info) };

        result
            .map(|_| file_info.nNumberOfLinks > 1)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }
}

// ---------------------------------------------------------------------------
// Unix implementation — uses metadata::nlink() (st_nlink from stat)
// ---------------------------------------------------------------------------
#[cfg(any(target_os = "linux", target_os = "macos"))]
impl<P: AsRef<Path>> IsHardLink for P {
    fn is_hardlink(&self) -> io::Result<bool> {
        use std::os::unix::fs::MetadataExt;

        let metadata = std::fs::metadata(self)?;
        Ok(metadata.nlink() > 1)
    }
}
