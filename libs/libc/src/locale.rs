use core::ffi::{c_char, c_int, c_void};

#[repr(C)]
pub struct lconv {
    pub decimal_point: *mut c_char,
    pub thousands_sep: *mut c_char,
    pub grouping: *mut c_char,
    pub int_curr_symbol: *mut c_char,
    pub currency_symbol: *mut c_char,
    pub mon_decimal_point: *mut c_char,
    pub mon_thousands_sep: *mut c_char,
    pub mon_grouping: *mut c_char,
    pub positive_sign: *mut c_char,
    pub negative_sign: *mut c_char,
    pub int_frac_digits: c_char,
    pub frac_digits: c_char,
    pub p_cs_precedes: c_char,
    pub p_sep_by_space: c_char,
    pub n_cs_precedes: c_char,
    pub n_sep_by_space: c_char,
    pub p_sign_posn: c_char,
    pub n_sign_posn: c_char,
}

static mut GERMAN_LOCALE: lconv = lconv {
    decimal_point: b",\0".as_ptr() as *mut c_char,
    thousands_sep: b".\0".as_ptr() as *mut c_char,
    grouping: b"\x03\0".as_ptr() as *mut c_char,
    int_curr_symbol: b"EUR \0".as_ptr() as *mut c_char,
    currency_symbol: b"\xe2\x82\xac\0".as_ptr() as *mut c_char,
    mon_decimal_point: b",\0".as_ptr() as *mut c_char,
    mon_thousands_sep: b".\0".as_ptr() as *mut c_char,
    mon_grouping: b"\x03\0".as_ptr() as *mut c_char,
    positive_sign: b"\0".as_ptr() as *mut c_char,
    negative_sign: b"-\0".as_ptr() as *mut c_char,
    int_frac_digits: 2,
    frac_digits: 2,
    p_cs_precedes: 0,
    p_sep_by_space: 1,
    n_cs_precedes: 0,
    n_sep_by_space: 1,
    p_sign_posn: 1,
    n_sign_posn: 1,
};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(_category: c_int, _locale: *const c_char) -> *mut c_char {
    // Return a dummy string representing German locale to satisfy callers
    b"de_DE.UTF-8\0".as_ptr() as *mut c_char
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn localeconv() -> *mut lconv {
    &raw mut GERMAN_LOCALE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn nl_langinfo(_item: c_int) -> *mut c_char {
    b"\0".as_ptr() as *mut c_char
}
