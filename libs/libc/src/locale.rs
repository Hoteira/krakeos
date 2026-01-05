use core::ffi::{c_int, c_char, c_void};

#[unsafe(no_mangle)] pub unsafe extern "C" fn setlocale(_category: c_int, _locale: *const c_char) -> *mut c_char { 
    core::ptr::null_mut() 
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn localeconv() -> *mut c_void { 
    core::ptr::null_mut() 
}

#[unsafe(no_mangle)] pub unsafe extern "C" fn nl_langinfo(_item: c_int) -> *mut c_char { 
    b"\0".as_ptr() as *mut c_char 
}
