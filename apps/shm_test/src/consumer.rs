#![no_std]
#![no_main]

extern crate alloc;
use std::os;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    os::debug_print("SHM Consumer: Starting...\n");

    // Request existing shared memory named "test_shm"
    if let Some(ptr) = os::shm_get("test_shm", 4096) {
        os::debug_print("SHM Consumer: Got pointer. Reading message...\n");

        unsafe {
            let mut curr = ptr;
            let mut buf = [0u8; 64];
            let mut i = 0;
            while *curr != 0 && i < 63 {
                buf[i] = *curr;
                curr = curr.add(1);
                i += 1;
            }

            let msg = core::str::from_utf8(&buf[..i]).unwrap_or("Invalid UTF-8");
            os::debug_print("SHM Consumer: Message received: ");
            os::debug_print(msg);
            os::debug_print("\n");
        }
    } else {
        os::debug_print("SHM Consumer: Failed to get SHM. Did the producer run?\n");
    }

    0
}
