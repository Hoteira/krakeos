use crate::sys::krake_get_time_ms;
use core::ffi::c_long;

#[repr(C)]
pub struct tm {
    pub tm_sec: i32,
    pub tm_min: i32,
    pub tm_hour: i32,
    pub tm_mday: i32,
    pub tm_mon: i32,
    pub tm_year: i32,
    pub tm_wday: i32,
    pub tm_yday: i32,
    pub tm_isdst: i32,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn clock() -> c_long {
    krake_get_time_ms() as c_long
}

static mut STATIC_TM: tm = tm {
    tm_sec: 0,
    tm_min: 0,
    tm_hour: 0,
    tm_mday: 1,
    tm_mon: 0,
    tm_year: 70, // 1970
    tm_wday: 4,
    tm_yday: 0,
    tm_isdst: 0,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mktime(_tm: *mut tm) -> c_long {
    0 // Stub
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn gmtime(_timep: *const c_long) -> *mut tm {
    &raw mut STATIC_TM
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime(_timep: *const c_long) -> *mut tm {
    &raw mut STATIC_TM
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn strftime(s: *mut core::ffi::c_char, max: usize, format: *const core::ffi::c_char, _tm: *const tm) -> usize {
    if max > 0 {
        *s = 0;
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn difftime(time1: c_long, time0: c_long) -> f64 {
    (time1 - time0) as f64
}
