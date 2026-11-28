use crate::{Error, Result};
use std::{fs, path::Path};

pub(super) fn checkpoint(cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() {
        return Err(Error::Code("cancelled"));
    }
    Ok(())
}

// Windows 的复制接口也负责复制备用数据流（包括互联网来源标记）。
#[cfg(windows)]
pub(super) fn copy_file(src: &Path, dest: &Path, cancelled: &dyn Fn() -> bool) -> Result<()> {
    use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_REQUEST_ABORTED, HANDLE},
        Storage::FileSystem::{CopyFileExW, COPY_FILE_FAIL_IF_EXISTS, PROGRESS_CANCEL, PROGRESS_CONTINUE},
    };

    unsafe extern "system" fn progress(
        _: i64, _: i64, _: i64, _: i64, _: u32, _: u32, _: HANDLE, _: HANDLE, context: *const c_void,
    ) -> u32 {
        // CopyFileExW 同步调用，context 指向仍在作用域里的取消检查。
        let cancelled = unsafe { &*context.cast::<&dyn Fn() -> bool>() };
        if cancelled() { PROGRESS_CANCEL } else { PROGRESS_CONTINUE }
    }

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut encoded: Vec<_> = path.as_os_str().encode_wide().collect();
        if encoded.len() >= 248 {
            let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
            let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
            encoded = fs::canonicalize(parent)?.join(name).as_os_str().encode_wide().collect();
        }
        encoded.push(0);
        Ok(encoded)
    }

    checkpoint(cancelled)?;
    let (src, dest) = (wide(src)?, wide(dest)?);
    let result = unsafe {
        CopyFileExW(src.as_ptr(), dest.as_ptr(), Some(progress), ptr::from_ref(&cancelled).cast(), ptr::null_mut(), COPY_FILE_FAIL_IF_EXISTS)
    };
    if result == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_REQUEST_ABORTED as i32) {
            checkpoint(cancelled)?;
        }
        return Err(error.into());
    }
    checkpoint(cancelled)
}

#[cfg(not(windows))]
pub(super) fn copy_file(src: &Path, dest: &Path, cancelled: &dyn Fn() -> bool) -> Result<()> {
    use std::io::{Read, Write};

    checkpoint(cancelled)?;
    let mut input = fs::File::open(src)?;
    let mut output = fs::File::create_new(dest)?;
    let mut buffer = [0; 128 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        checkpoint(cancelled)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::fd::AsRawFd;
        // 数据已经分块复制，另外保留 macOS 的权限和扩展属性。
        let result = unsafe { libc::fcopyfile(input.as_raw_fd(), output.as_raw_fd(), std::ptr::null_mut(), libc::COPYFILE_METADATA) };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    output.set_permissions(input.metadata()?.permissions())?;

    checkpoint(cancelled)
}

pub(super) fn remove_staging(path: &Path) {
    #[cfg(windows)]
    for entry in walkdir::WalkDir::new(path).follow_links(false).into_iter().flatten().filter(|entry| entry.file_type().is_file()) {
        if let Ok(meta) = entry.metadata() {
            let mut permissions = meta.permissions();
            if permissions.readonly() {
                permissions.set_readonly(false);
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
    let _ = fs::remove_dir_all(path);
}
