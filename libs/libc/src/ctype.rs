use core::ffi::c_int;

#[unsafe(no_mangle)] pub unsafe extern "C" fn toupper(c: c_int) -> c_int { if c >= b'a' as c_int && c <= b'z' as c_int { c - 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn tolower(c: c_int) -> c_int { if c >= b'A' as c_int && c <= b'Z' as c_int { c + 32 } else { c } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isspace(c: c_int) -> c_int { if c == b' ' as c_int || c == b'\t' as c_int || c == b'\n' as c_int || c == b'\r' as c_int || c == 0x0B || c == 0x0C { 1 } else { 0 } }
#[unsafe(no_mangle)] pub unsafe extern "C" fn isdigit(c: c_int) -> c_int { if c >= b'0' as c_int && c <= b'9' as c_int { 1 } else { 0 } }

#[unsafe(no_mangle)] pub unsafe extern "C" fn isalpha(c: c_int) -> c_int { 
    if (c >= b'a' as c_int && c <= b'z' as c_int) || (c >= b'A' as c_int && c <= b'Z' as c_int) { 1 } else { 0 } 
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isalnum(c: c_int) -> c_int { 
    if isalpha(c) != 0 || isdigit(c) != 0 { 1 } else { 0 } 
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isblank(c: c_int) -> c_int { 
    if c == b' ' as c_int || c == b'\t' as c_int { 1 } else { 0 } 
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn iscntrl(c: c_int) -> c_int {
    if (c >= 0 && c < 32) || c == 127 { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isgraph(c: c_int) -> c_int {
    if c > 32 && c < 127 { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn islower(c: c_int) -> c_int {
    if c >= b'a' as c_int && c <= b'z' as c_int { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isprint(c: c_int) -> c_int {
    if c >= 32 && c < 127 { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn ispunct(c: c_int) -> c_int {
    if isgraph(c) != 0 && isalnum(c) == 0 { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isxdigit(c: c_int) -> c_int {
    if isdigit(c) != 0 || (c >= b'a' as c_int && c <= b'f' as c_int) || (c >= b'A' as c_int && c <= b'F' as c_int) { 1 } else { 0 }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn isupper(c: c_int) -> c_int {
    if c >= b'A' as c_int && c <= b'Z' as c_int { 1 } else { 0 }
}
