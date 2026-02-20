#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:core/system@0.2.0")]
unsafe extern "C" {
    #[link_name = "syscall"]
    pub fn krakeos_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64;
    #[link_name = "syscall5"]
    pub fn krakeos_syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64;
    #[link_name = "syscall6"]
    pub fn krakeos_syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64;
    #[link_name = "syscall7"]
    pub fn krakeos_syscall7(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_syscall(num: u64, arg1: u64, arg2: u64, arg3: u64) -> u64 {
    crate::sys::syscall(num, arg1, arg2, arg3)
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_syscall5(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    crate::sys::syscall4(num, arg1, arg2, arg3, arg4)
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_syscall6(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> u64 {
    crate::sys::syscall5(num, arg1, arg2, arg3, arg4, arg5)
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_syscall7(num: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> u64 {
    crate::sys::syscall6(num, arg1, arg2, arg3, arg4, arg5, arg6)
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:graphics/screen@0.2.0")]
unsafe extern "C" {
    #[link_name = "get-width"]
    pub fn get_screen_width() -> u32;
    #[link_name = "get-height"]
    pub fn get_screen_height() -> u32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn get_screen_width() -> u32 {
    crate::os::graphics::get_screen_width() as u32
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn get_screen_height() -> u32 {
    crate::os::graphics::get_screen_height() as u32
}

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "krakeos:net/raw@0.2.0")]
unsafe extern "C" {
    #[link_name = "send"]
    pub fn krakeos_net_send(ptr: *const u8, len: u32) -> i32;
    #[link_name = "recv"]
    pub fn krakeos_net_recv(ptr: *mut u8, len: u32) -> i32;
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_net_send(_ptr: *const u8, _len: u32) -> i32 {
    -1 // Deprecated
}

#[cfg(target_arch = "x86_64")]
pub unsafe fn krakeos_net_recv(_ptr: *mut u8, _len: u32) -> i32 {
    0 // Deprecated
}