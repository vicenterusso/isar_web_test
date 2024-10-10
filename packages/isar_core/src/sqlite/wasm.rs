use libsqlite3_sys::{sqlite3_file, sqlite3_vfs, sqlite3_vfs_register, sqlite3_io_methods, SQLITE_OK, SQLITE_BUSY, SQLITE_IOERR};
use std::sync::atomic::{AtomicI32, Ordering};
use std::os::raw::{c_char, c_int, c_void};
use wasm_bindgen::prelude::*;
use std::ptr::null_mut;
use js_sys::Date;

static mut OPFS_IO_METHODS: *mut sqlite3_io_methods = std::ptr::null_mut();

// JavaScript functions we'll call from Rust
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window)]
    fn opfs_open(path: &str, flags: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_close(fd: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_read(fd: i32, buffer: &mut [u8], offset: i64, len: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_write(fd: i32, buffer: &[u8], offset: i64) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_truncate(fd: i32, size: i64) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_sync(fd: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_file_size(fd: i32) -> i64;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_delete(path: &str) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_access(path: &str, flags: i32) -> i32;
}

// Custom file structure for our VFS
#[repr(C)]
struct OpfsFile {
    base: sqlite3_file,
    fd: i32,
    lock_state: AtomicI32,
}

const SQLITE_LOCK_NONE: i32 = 0;
const SQLITE_LOCK_SHARED: i32 = 1;
const SQLITE_LOCK_RESERVED: i32 = 2;
const SQLITE_LOCK_PENDING: i32 = 3;
const SQLITE_LOCK_EXCLUSIVE: i32 = 4;

// VFS functions
unsafe extern "C" fn opfs_vfs_open(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    file: *mut sqlite3_file,
    flags: c_int,
    _out_flags: *mut c_int,
) -> c_int {
    let path = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    let fd = opfs_open(path, flags);
    if fd < 0 {
        return SQLITE_IOERR;
    }
    let opfs_file = &mut *(file as *mut OpfsFile);
    opfs_file.fd = fd;
    opfs_file.lock_state = AtomicI32::new(SQLITE_LOCK_NONE);
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_close(file: *mut sqlite3_file) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    match opfs_close(opfs_file.fd) {
        0 => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_read(
    file: *mut sqlite3_file,
    buf: *mut c_void,
    count: c_int,
    offset: i64,
) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let buffer = std::slice::from_raw_parts_mut(buf as *mut u8, count as usize);
    match opfs_read(opfs_file.fd, buffer, offset, count) {
        n if n == count => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_write(
    file: *mut sqlite3_file,
    buf: *const c_void,
    count: c_int,
    offset: i64,
) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let buffer = std::slice::from_raw_parts(buf as *const u8, count as usize);
    match opfs_write(opfs_file.fd, buffer, offset) {
        n if n == count => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_truncate(file: *mut sqlite3_file, size: i64) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    match opfs_truncate(opfs_file.fd, size) {
        0 => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_sync(file: *mut sqlite3_file, _flags: c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    match opfs_sync(opfs_file.fd) {
        0 => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_file_size(
    file: *mut sqlite3_file,
    size: *mut i64,
) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    *size = opfs_file_size(opfs_file.fd);
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_lock(file: *mut sqlite3_file, lock: c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    
    if current_lock < lock {
        opfs_file.lock_state.store(lock, Ordering::Relaxed);
        SQLITE_OK
    } else {
        SQLITE_BUSY
    }
}

unsafe extern "C" fn opfs_vfs_unlock(file: *mut sqlite3_file, lock: c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    
    if current_lock > lock {
        opfs_file.lock_state.store(lock, Ordering::Relaxed);
    }
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_check_reserved_lock(file: *mut sqlite3_file, res_out: *mut c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    *res_out = if current_lock >= SQLITE_LOCK_RESERVED { 1 } else { 0 };
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_file_control(_file: *mut sqlite3_file, _op: c_int, _arg: *mut c_void) -> c_int {
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_sector_size(_file: *mut sqlite3_file) -> c_int {
    4096 // Default sector size
}

unsafe extern "C" fn opfs_vfs_device_characteristics(_file: *mut sqlite3_file) -> c_int {
    0 // No special device characteristics
}

unsafe extern "C" fn opfs_vfs_delete(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    let path = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    match opfs_delete(path) {
        0 => SQLITE_OK,
        _ => SQLITE_IOERR,
    }
}

unsafe extern "C" fn opfs_vfs_access(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    flags: c_int,
    res_out: *mut c_int,
) -> c_int {
    let path = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    *res_out = opfs_access(path, flags);
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_full_pathname(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    nout: c_int,
    zout: *mut c_char,
) -> c_int {
    let input = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    let output = std::ffi::CStr::from_bytes_with_nul(input.as_bytes())
        .unwrap()
        .to_owned();
    let len = std::cmp::min(nout as usize - 1, output.as_bytes().len());
    std::ptr::copy_nonoverlapping(output.as_ptr(), zout, len);
    *zout.add(len) = 0;
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_randomness(_vfs: *mut sqlite3_vfs, nbytes: c_int, zbytes: *mut c_char) -> c_int {
    // This is a simplified version. In a real implementation, you'd want to use a proper source of randomness.
    for i in 0..nbytes as usize {
        *zbytes.add(i) = (i % 256) as i8;
    }
    nbytes
}

unsafe extern "C" fn opfs_vfs_sleep(_vfs: *mut sqlite3_vfs, microseconds: c_int) -> c_int {
    // We can't actually sleep in WebAssembly, so we just return
    microseconds / 1000
}

unsafe extern "C" fn opfs_vfs_current_time(_vfs: *mut sqlite3_vfs, time: *mut f64) -> c_int {
    *time = Date::now() / 86400000.0 + 2440587.5;
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_get_last_error(_vfs: *mut sqlite3_vfs, _n: c_int, _z: *mut c_char) -> c_int {
    0 // We don't implement this for now
}

#[no_mangle]
pub unsafe extern "C" fn sqlite3_os_init() -> c_int {
    let opfs_vfs = sqlite3_vfs {
        iVersion: 1,
        szOsFile: std::mem::size_of::<OpfsFile>() as c_int,
        mxPathname: 1024,
        pNext: null_mut(),
        zName: b"opfs\0".as_ptr() as *const c_char,
        pAppData: null_mut(),
        xOpen: Some(opfs_vfs_open),
        xDelete: Some(opfs_vfs_delete),
        xAccess: Some(opfs_vfs_access),
        xFullPathname: Some(opfs_vfs_full_pathname),
        xDlOpen: None,
        xDlError: None,
        xDlSym: None,
        xDlClose: None,
        xRandomness: Some(opfs_vfs_randomness),
        xSleep: Some(opfs_vfs_sleep),
        xCurrentTime: Some(opfs_vfs_current_time),
        xGetLastError: Some(opfs_vfs_get_last_error),
        xCurrentTimeInt64: None,
        xSetSystemCall: None,
        xGetSystemCall: None,
        xNextSystemCall: None,
    };

    let opfs_io_methods = sqlite3_io_methods {
        iVersion: 1,
        xClose: Some(opfs_vfs_close),
        xRead: Some(opfs_vfs_read),
        xWrite: Some(opfs_vfs_write),
        xTruncate: Some(opfs_vfs_truncate),
        xSync: Some(opfs_vfs_sync),
        xFileSize: Some(opfs_vfs_file_size),
        xLock: Some(opfs_vfs_lock),
        xUnlock: Some(opfs_vfs_unlock),
        xCheckReservedLock: Some(opfs_vfs_check_reserved_lock),
        xFileControl: Some(opfs_vfs_file_control),
        xSectorSize: Some(opfs_vfs_sector_size),
        xDeviceCharacteristics: Some(opfs_vfs_device_characteristics),
        xShmMap: None,
        xShmLock: None,
        xShmBarrier: None,
        xShmUnmap: None,
        xFetch: None,
        xUnfetch: None,
    };

    OPFS_IO_METHODS = Box::into_raw(Box::new(opfs_io_methods));
    
    sqlite3_vfs_register(Box::leak(Box::new(opfs_vfs)), 1)
}

#[no_mangle]
pub unsafe extern "C" fn sqlite3_os_end() {
    // Clean up any resources if necessary
    if !OPFS_IO_METHODS.is_null() {
        drop(Box::from_raw(OPFS_IO_METHODS));
    }
}