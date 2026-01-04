use core::ffi::{c_char, c_int};
use crate::string::{strcmp};

#[unsafe(no_mangle)] pub static mut optarg: *mut c_char = core::ptr::null_mut();
#[unsafe(no_mangle)] pub static mut optind: c_int = 1;
#[unsafe(no_mangle)] pub static mut opterr: c_int = 1;
#[unsafe(no_mangle)] pub static mut optopt: c_int = 0;

#[repr(C)]
pub struct option {
    pub name: *const c_char,
    pub has_arg: c_int,
    pub flag: *mut c_int,
    pub val: c_int,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getopt(argc: c_int, argv: *const *mut c_char, optstring: *const c_char) -> c_int {
    getopt_long(argc, argv, optstring, core::ptr::null(), core::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn getopt_long(argc: c_int, argv: *const *mut c_char, optstring: *const c_char, longopts: *const option, _longindex: *mut c_int) -> c_int {
    if optind >= argc { return -1; }
    let arg = *argv.add(optind as usize);
    if arg.is_null() || *arg != b'-' as c_char || *arg.add(1) == 0 { return -1; }

    if *arg.add(1) == b'-' as c_char {
        if longopts.is_null() { return -1; }
        let name = arg.add(2);
        let mut i = 0;
        while !(*longopts.add(i)).name.is_null() {
            let opt = &*longopts.add(i);
            if strcmp(name, opt.name) == 0 {
                optind += 1;
                if opt.has_arg != 0 {
                    if optind < argc {
                        optarg = *argv.add(optind as usize);
                        optind += 1;
                    } else { return b'?' as c_int; }
                }
                if opt.flag.is_null() { return opt.val; } 
                else { *opt.flag = opt.val; return 0; }
            }
            i += 1;
        }
        return b'?' as c_int;
    }

    let c = *arg.add(1);
    let mut p = optstring;
    while *p != 0 {
        if *p == c {
            optind += 1;
            if *p.add(1) == b':' as c_char {
                if optind < argc {
                    optarg = *argv.add(optind as usize);
                    optind += 1;
                } else { return b'?' as c_int; }
            }
            return c as c_int;
        }
        p = p.add(1);
    }
    b'?' as c_int
}
