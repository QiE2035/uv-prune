use std::{fs::File, path::Path, result::Result};

pub trait IsHardLink<E = std::io::Error> {
    fn is_hardlink(&self) -> Result<bool, E>;
}

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;

#[cfg(target_os = "windows")]
use windows::{
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    },
    core::Error,
};

#[cfg(target_os = "windows")]
impl<P: AsRef<Path>> IsHardLink<Error> for P {
    fn is_hardlink(&self) -> Result<bool, Error> {
        let file = File::open(self)?;
        let handle = HANDLE(file.as_raw_handle());

        // 准备接收文件信息
        let mut file_info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };

        // 调用 Windows API 获取文件信息
        let result = unsafe { GetFileInformationByHandle(handle, &mut file_info) };

        if let Err(err) = result {
            Err(err)
        } else {
            // 检查链接计数（nNumberOfLinks）
            Ok(file_info.nNumberOfLinks > 1)
        }
    }
}
