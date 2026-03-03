#![no_std]

extern crate alloc;
use std::os;

pub fn main() {
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
}
