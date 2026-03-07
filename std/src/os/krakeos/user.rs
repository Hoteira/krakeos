use crate::sync::Mutex;
use crate::alloc::string::String;

static CURRENT_USER: Mutex<Option<String>> = Mutex::new(None);

method_export!("krakeos:system/process@0.2.0", "get-current-user",
    pub fn raw_get_current_user(ret_ptr: *mut u8) {
        // Native fallback
        let user = "racap";
        unsafe {
            #[cfg(target_arch = "wasm32")]
            {
                // WASM32: ptr@0 (4 bytes), len@4 (4 bytes)
                core::ptr::write_unaligned(ret_ptr as *mut u32, user.as_ptr() as u32);
                core::ptr::write_unaligned(ret_ptr.add(4) as *mut u32, user.len() as u32);
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                // x86_64: ptr@0 (8 bytes), len@8 (8 bytes)
                core::ptr::write_unaligned(ret_ptr as *mut u64, user.as_ptr() as u64);
                core::ptr::write_unaligned(ret_ptr.add(8) as *mut u64, user.len() as u64);
            }
        }
    }
);

pub fn get_current_user() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let mut ret = [0u8; 8];
        raw_get_current_user(ret.as_mut_ptr());
        let ptr = unsafe { core::ptr::read_unaligned(ret.as_ptr() as *const u32) };
        let len = unsafe { core::ptr::read_unaligned(ret.as_ptr().add(4) as *const u32) };
        let slice = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
        String::from_utf8_lossy(slice).into_owned()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let user = CURRENT_USER.lock();
        user.clone().unwrap_or_else(|| String::from("racap"))
    }
}

pub fn set_current_user(name: &str) {
    let mut user = CURRENT_USER.lock();
    *user = Some(String::from(name.trim()));
}
