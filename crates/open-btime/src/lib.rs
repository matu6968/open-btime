use neon::prelude::*;
use neon::types::buffer::TypedArray;

// Set the birth time (creation time) of a file
fn btime(mut cx: FunctionContext) -> JsResult<JsNumber> {
    // Extract parameters
    if cx.len() < 2 {
        return cx.throw_error("bad arguments, expected: (buffer path, seconds btime)");
    }
    
    // Get the buffer containing the path
    let path_buffer = cx.argument::<JsBuffer>(0)?;
    let path_bytes = path_buffer.as_slice(&cx).to_vec();
    
    // Find the null terminator
    let null_pos = path_bytes.iter().position(|&b| b == 0)
        .unwrap_or(path_bytes.len());
    
    // Convert to a UTF-8 string up to the null terminator
    let path_str = match std::str::from_utf8(&path_bytes[0..null_pos]) {
        Ok(s) => s,
        Err(_) => return cx.throw_error("Invalid UTF-8 in path"),
    };
    
    // Get the btime seconds parameter
    let btime_seconds = cx.argument::<JsNumber>(1)?.value(&mut cx) as u64;
    
    // Try to set the birth time
    match set_btime(path_str, btime_seconds) {
        Ok(_) => Ok(cx.number(0)), // Return 0 on success (like the original C++ implementation)
        Err(err) => {
            let error_message = format!("({}) utimes({})", err.raw_os_error().unwrap_or(-1), path_str);
            cx.throw_error(error_message)
        }
    }
}

// Platform-specific implementation of setting birth time
#[cfg(target_os = "windows")]
fn set_btime(path: &str, seconds: u64) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{SetFileTime, FILE_WRITE_ATTRIBUTES};
    
    // Convert Unix timestamp to Windows FILETIME
    let intervals = seconds * 10_000_000 + 116_444_736_000_000_000;
    let ft = FILETIME {
        dwLowDateTime: (intervals & 0xFFFFFFFF) as u32,
        dwHighDateTime: (intervals >> 32) as u32,
    };
    
    // Open the file with write attributes permission
    let file = OpenOptions::new()
        .write(true)
        .custom_flags(FILE_WRITE_ATTRIBUTES.0)
        .open(path)?;
    
    // Get the file handle
    let handle = HANDLE(file.as_raw_handle() as isize);
    
    // Set the creation time (birth time)
    let result = unsafe { SetFileTime(handle, Some(&ft), None, None) };
    
    if !result.as_bool() {
        return Err(std::io::Error::last_os_error());
    }
    
    // The file is closed automatically when it goes out of scope
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_btime(path: &str, seconds: u64) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::raw::{c_char, c_int};
    use std::path::PathBuf;
    
    // Create C-compatible path string
    let path_buf = PathBuf::from(path);
    let c_path = CString::new(path_buf.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "Path contains null bytes"))?;
    
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }
    
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Timeval {
        tv_sec: i64,
        tv_usec: i32,
    }
    
    #[repr(C)]
    struct AttrList {
        bitmapcount: u16,
        reserved: u16,
        commonattr: u32,
        volattr: u32,
        dirattr: u32,
        fileattr: u32,
        forkattr: u32,
    }
    
    #[repr(C)]
    struct AttrBuf {
        ret_length: u32,
        struct_length: u32,
        btime: Timespec,
    }
    
    const ATTR_BIT_MAP_COUNT: u16 = 5;
    const ATTR_CMN_CRTIME: u32 = 0x00000200;
    
    extern "C" {
        fn setattrlist(
            path: *const c_char,
            attrList: *const AttrList,
            attrBuf: *const libc::c_void,
            attrBufSize: libc::size_t,
            options: c_int,
        ) -> c_int;
    }
    
    // Prepare the attribute list
    let mut attr_list = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_CRTIME,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    
    // Prepare the attribute buffer with the birth time
    let attr_buf = AttrBuf {
        ret_length: 0,
        struct_length: 0,
        btime: Timespec {
            tv_sec: seconds as i64,
            tv_nsec: 0,
        },
    };
    
    // Call setattrlist
    let result = unsafe {
        setattrlist(
            c_path.as_ptr(),
            &mut attr_list as *mut AttrList,
            &attr_buf as *const AttrBuf as *const libc::c_void,
            std::mem::size_of::<AttrBuf>(),
            0,
        )
    };
    
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn set_btime(_path: &str, _seconds: u64) -> std::io::Result<()> {
    // Linux does not support changing birth time
    Ok(())
}

// Update the Cargo.toml for platform-specific dependencies:
// For Windows:
// windows = { version = "0.51", features = ["Win32_Foundation", "Win32_Storage_FileSystem"] }
// For macOS:
// libc = "0.2"

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("btime", btime)?;
    Ok(())
}
