#![no_std]
#![no_main]

extern crate alloc;
use std::os;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    os::debug_print("SHM Producer: Starting...\n");

    // Request 4KB of shared memory named "test_shm"
    if let Some(addr) = os::shm_get("test_shm", 4096) {
        os::debug_print("SHM Producer: Got pointer. Writing message...\n");

        let ptr = addr as *mut u8;
        let message = b"Hello from SAS Shared Memory!";
        unsafe {
            core::ptr::copy_nonoverlapping(message.as_ptr(), ptr, message.len());
            // Null terminator
            *ptr.add(message.len()) = 0;
        }

        os::debug_print("SHM Producer: Done. Sleeping 5s...\n");
        os::sleep(5000);
    } else {
        os::debug_print("SHM Producer: Failed to get SHM.\n");
    }

    0
}
