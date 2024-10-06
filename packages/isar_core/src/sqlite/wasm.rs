use libsqlite3_sys::{sqlite3_file, sqlite3_vfs, sqlite3_vfs_register, SQLITE_IOERR};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr::null_mut;

extern "C" {
    /*pub fn js_log(ptr: *const u8);

    pub fn xSleep(_arg1: *mut sqlite3_vfs, microseconds: c_int) -> c_int;

    pub fn xRandomness(_arg1: *mut sqlite3_vfs, nByte: c_int, zByte: *mut c_char) -> c_int;

    pub fn xCurrentTime(_arg1: *mut sqlite3_vfs, pTime: *mut f64) -> c_int;*/
}

use libsqlite3_sys::{sqlite3_file, sqlite3_vfs, sqlite3_vfs_register, SQLITE_IOERR};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr::null_mut;
use wasm_bindgen::prelude::*;

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
}

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
    // Initialize other file methods here
    0 // SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_delete(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    _sync_dir: c_int,
) -> c_int {
    let path = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    opfs_delete(path)
}

unsafe extern "C" fn opfs_vfs_access(
    _vfs: *mut sqlite3_vfs,
    zname: *const c_char,
    flags: c_int,
    res_out: *mut c_int,
) -> c_int {
    let path = std::ffi::CStr::from_ptr(zname).to_str().unwrap();
    *res_out = opfs_access(path, flags);
    0 // SQLITE_OK
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
        xFullPathname: Some(opfs_vfs_fullpathname),
        xDlOpen: None,
        xDlError: None,
        xDlSym: None,
        xDlClose: None,
        xRandomness: Some(opfs_vfs_randomness),
        xSleep: Some(opfs_vfs_sleep),
        xCurrentTime: Some(opfs_vfs_current_time),
        xGetLastError: None,
        xCurrentTimeInt64: None,
        xSetSystemCall: None,
        xGetSystemCall: None,
        xNextSystemCall: None,
    };
    
    sqlite3_vfsv_register(Box::leak(Box::new(opfs_vfs)), 1)
}

pub unsafe extern "C" fn xSleep(_arg1: *mut sqlite3_vfs, microseconds: c_int) -> c_int {
    0
}

pub unsafe extern "C" fn xRandomness(
    _arg1: *mut sqlite3_vfs,
    nByte: c_int,
    zByte: *mut c_char,
) -> c_int {
    0
}

pub unsafe extern "C" fn xCurrentTime(_arg1: *mut sqlite3_vfs, pTime: *mut f64) -> c_int {
    0
}

const fn max(a: usize, b: usize) -> usize {
    [a, b][(a < b) as usize]
}

const ALIGN: usize = max(
    8, // wasm32 max_align_t
    max(std::mem::size_of::<usize>(), std::mem::align_of::<usize>()),
);

#[no_mangle]
pub unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    let layout = match std::alloc::Layout::from_size_align(size + ALIGN, ALIGN) {
        Ok(layout) => layout,
        Err(_) => return null_mut(),
    };

    let ptr = std::alloc::alloc(layout);
    if ptr.is_null() {
        return null_mut();
    }

    *(ptr as *mut usize) = size;
    ptr.offset(ALIGN as isize)
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut u8) {
    let ptr = ptr.offset(-(ALIGN as isize));
    let size = *(ptr as *mut usize);
    let layout = std::alloc::Layout::from_size_align_unchecked(size + ALIGN, ALIGN);

    std::alloc::dealloc(ptr, layout);
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut u8, new_size: usize) -> *mut u8 {
    let ptr = ptr.offset(-(ALIGN as isize));
    let size = *(ptr as *mut usize);
    let layout = std::alloc::Layout::from_size_align_unchecked(size + ALIGN, ALIGN);

    let ptr = std::alloc::realloc(ptr, layout, new_size + ALIGN);
    if ptr.is_null() {
        return null_mut();
    }

    *(ptr as *mut usize) = new_size;
    ptr.offset(ALIGN as isize)
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_open(
    _arg1: *mut sqlite3_vfs,
    _zName: *const c_char,
    _arg2: *mut sqlite3_file,
    _flags: c_int,
    _pOutFlags: *mut c_int,
) -> c_int {
    SQLITE_IOERR
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_delete(
    _arg1: *mut sqlite3_vfs,
    _zName: *const c_char,
    _syncDir: c_int,
) -> c_int {
    SQLITE_IOERR
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_access(
    _arg1: *mut sqlite3_vfs,
    _zName: *const c_char,
    _flags: c_int,
    _pResOut: *mut c_int,
) -> c_int {
    SQLITE_IOERR
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_fullpathname(
    _arg1: *mut sqlite3_vfs,
    _zName: *const c_char,
    _nOut: c_int,
    _zOut: *mut c_char,
) -> c_int {
    SQLITE_IOERR
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_dlopen(
    _arg1: *mut sqlite3_vfs,
    _zFilename: *const c_char,
) -> *mut c_void {
    null_mut()
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_dlerror(
    _arg1: *mut sqlite3_vfs,
    _nByte: c_int,
    _zErrMsg: *mut c_char,
) {
    // no-op
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_dlsym(
    _arg1: *mut sqlite3_vfs,
    _arg2: *mut c_void,
    _zSymbol: *const c_char,
) -> ::std::option::Option<unsafe extern "C" fn(*mut sqlite3_vfs, *mut c_void, *const i8)> {
    None
}

#[no_mangle]
unsafe extern "C" fn wasm_vfs_dlclose(_arg1: *mut sqlite3_vfs, _arg2: *mut c_void) {
    // no-op
}
