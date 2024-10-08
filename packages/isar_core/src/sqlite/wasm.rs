use libsqlite3_sys::{sqlite3_file, sqlite3_vfs, sqlite3_vfs_register, sqlite3_io_methods, SQLITE_OK, SQLITE_BUSY, SQLITE_IOERR};
use std::sync::atomic::{AtomicI32, Ordering};
use std::os::raw::{c_char, c_int, c_void};
use wasm_bindgen::prelude::*;
use std::ptr::null_mut;
use js_sys::Uint8Array;
use web_sys::window;
use std::slice;


extern "C" {
    /*pub fn js_log(ptr: *const u8);

    pub fn xSleep(_arg1: *mut sqlite3_vfs, microseconds: c_int) -> c_int;

    pub fn xRandomness(_arg1: *mut sqlite3_vfs, nByte: c_int, zByte: *mut c_char) -> c_int;

    pub fn xCurrentTime(_arg1: *mut sqlite3_vfs, pTime: *mut f64) -> c_int;*/
}


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

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_acquire_lock(fd: i32, lock: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_release_lock(fd: i32, lock: i32) -> i32;

    #[wasm_bindgen(js_namespace = window)]
    fn opfs_file_control(fd: i32, op: i32, arg: *mut c_void) -> i32;
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

unsafe extern "C" fn opfs_vfs_lock(file: *mut sqlite3_file, lock: c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    
    if current_lock < lock {
        match opfs_acquire_lock(opfs_file.fd, lock) {
            0 => {
                opfs_file.lock_state.store(lock, Ordering::Relaxed);
                SQLITE_OK
            },
            _ => SQLITE_BUSY
        }
    } else {
        SQLITE_OK
    }
}

unsafe extern "C" fn opfs_vfs_unlock(file: *mut sqlite3_file, lock: c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    
    if current_lock > lock {
        match opfs_release_lock(opfs_file.fd, lock) {
            0 => {
                opfs_file.lock_state.store(lock, Ordering::Relaxed);
                SQLITE_OK
            },
            _ => SQLITE_IOERR
        }
    } else {
        SQLITE_OK
    }
}

unsafe extern "C" fn opfs_vfs_check_reserved_lock(file: *mut sqlite3_file, res_out: *mut c_int) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    let current_lock = opfs_file.lock_state.load(Ordering::Relaxed);
    *res_out = if current_lock >= SQLITE_LOCK_RESERVED { 1 } else { 0 };
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_file_control(file: *mut sqlite3_file, op: c_int, arg: *mut c_void) -> c_int {
    let opfs_file = &mut *(file as *mut OpfsFile);
    match opfs_file_control(opfs_file.fd, op, arg) {
        0 => SQLITE_OK,
        _ => SQLITE_IOERR
    }
}

unsafe extern "C" fn opfs_vfs_sector_size(_file: *mut sqlite3_file) -> c_int {
    // OPFS doesn't have a concept of sectors, so we return a reasonable default
    4096
}

unsafe extern "C" fn opfs_vfs_device_characteristics(_file: *mut sqlite3_file) -> c_int {
    // Return characteristics that match OPFS capabilities
    // This is a simplified version and may need adjustment
    libsqlite3_sys::SQLITE_IOCAP_SAFE_APPEND | 
    libsqlite3_sys::SQLITE_IOCAP_SEQUENTIAL | 
    libsqlite3_sys::SQLITE_IOCAP_UNDELETABLE_WHEN_OPEN
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
    let buffer = slice::from_raw_parts_mut(buf as *mut u8, count as usize);
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
    let buffer = slice::from_raw_parts(buf as *const u8, count as usize);
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

unsafe extern "C" fn opfs_vfs_fullpathname(
    _arg1: *mut sqlite3_vfs,
    zName: *const c_char,
    nOut: c_int,
    zOut: *mut c_char,
) -> c_int {
    let input = std::ffi::CStr::from_ptr(zName).to_str().unwrap();
    let output = std::ffi::CStr::from_bytes_with_nul(input.as_bytes())
        .unwrap()
        .to_owned();
    let len = std::cmp::min(nOut as usize - 1, output.as_bytes().len());
    unsafe {
        std::ptr::copy_nonoverlapping(output.as_ptr(), zOut, len);
        *zOut.add(len) = 0;
    }
    SQLITE_OK
}

unsafe extern "C" fn opfs_vfs_randomness(
    _arg1: *mut sqlite3_vfs,
    nByte: c_int,
    zByte: *mut c_char,
) -> c_int {
    let window = match window() {
        Some(win) => win,
        None => return 0, // Return 0 if we can't get the window object
    };

    let crypto = match window.crypto() {
        Ok(crypto) => crypto,init_opfs 
        Err(_) => return 0, // Return 0 if we can't get the crypto object
    };

    let buffer = match Uint8Array::new_with_length(nByte as u32).into_js_result() {
        Ok(buf) => buf,
        Err(_) => return 0, // Return 0 if we can't create the buffer
    };

    if let Err(_) = crypto.get_random_values_with_u8_array(&buffer) {
        return 0; // Return 0 if we can't fill the buffer with random values
    }

    for i in 0..nByte as usize {
        *zByte.add(i) = buffer.get_index(i as u32) as i8;
    }

    nByte
}

unsafe extern "C" fn opfs_vfs_sleep(
    _arg1: *mut sqlite3_vfs,
    microseconds: c_int,
) -> c_int {
    // In a web environment, we can't actually sleep.
    // We'll just return immediately.
    0
}

unsafe extern "C" fn opfs_vfs_current_time(
    _arg1: *mut sqlite3_vfs,
    pTime: *mut f64,
) -> c_int {
    unsafe {
        *pTime = js_sys::Date::now() / 86400000.0 + 2440587.5;
    }
    SQLITE_OK
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

    let opfs_io_methods = sqlite3_io_methods {
        iVersion: 1,
        xFetch: Some(opfs_io_fetch),
        xShmBarrier: Some(opfs_io_shm_barrier),
        xShmLock: Some(opfs_io_shm_lock),
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
        // ... fill in the rest of the methods as needed
    };

    // Store opfs_io_methods in a static variable or some other way to keep it alive
    OPFS_IO_METHODS = Box::into_raw(Box::new(opfs_io_methods));
    
    sqlite3_vfs_register(Box::leak(Box::new(opfs_vfs)), 1)
}

pub unsafe extern "C" fn xSleep(_arg1: *mut sqlite3_vfs, _microseconds: c_int) -> c_int {
    0
}

pub unsafe extern "C" fn xRandomness(
    _arg1: *mut sqlite3_vfs,
    nByte: c_int,
    zByte: *mut c_char,
) -> c_int {
    0
}

pub unsafe extern "C" fn xCurrentTime(_arg1: *mut sqlite3_vfs, _pTime: *mut f64) -> c_int {
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

