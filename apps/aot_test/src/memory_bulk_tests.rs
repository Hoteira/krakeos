use std::println;

#[cfg(target_arch = "wasm32")]
use core::arch::asm;

pub fn test_memory_bulk() -> i32 {
    println!("==================== Testing Memory Bulk Operations...");
    let mut errors = 0;

    errors += test_memory_fill();
    errors += test_memory_copy();
    errors += test_memory_management();
    errors += test_memory_opcodes_direct();

    if errors == 0 { println!("==================== Memory Bulk: OK"); }
    errors
}

#[cfg(target_arch = "wasm32")]
fn test_memory_opcodes_direct() -> i32 {
    let mut errors = 0;
    let mut buf = [0u8; 100];
    
    unsafe {
        // memory.fill pops [i32 (offset), i32 (val), i32 (count)]
        asm!(
            "local.get {0}", // offset
            "local.get {1}", // val
            "local.get {2}", // count
            "memory.fill 0",
            in(local) buf.as_mut_ptr() as i32,
            in(local) 0xCC,
            in(local) 10,
        );
        
        for i in 0..10 {
            if buf[i] != 0xCC {
                println!("Error: direct memory.fill failed at index {}", i);
                errors += 1;
                break;
            }
        }
        
        // memory.copy pops [i32 (dest), i32 (src), i32 (count)]
        asm!(
            "local.get {0}", // dest
            "local.get {1}", // src
            "local.get {2}", // count
            "memory.copy 0, 0",
            in(local) (buf.as_mut_ptr() as i32 + 50),
            in(local) buf.as_mut_ptr() as i32,
            in(local) 10,
        );
        
        for i in 0..10 {
            if buf[i+50] != 0xCC {
                println!("Error: direct memory.copy failed at index {}", i+50);
                errors += 1;
                break;
            }
        }
    }
    
    errors
}

#[cfg(not(target_arch = "wasm32"))]
fn test_memory_opcodes_direct() -> i32 { 0 }

fn test_memory_fill() -> i32 {
    let mut errors = 0;
    let mut buf = [0u8; 1024];
    
    // Fill entire buffer with 0xAA (lowers to memory.fill)
    unsafe { core::ptr::write_bytes(buf.as_mut_ptr(), 0xAA, 1024); }
    for i in 0..1024 {
        if buf[i] != 0xAA {
            println!("Error: memory.fill failed at index {}", i);
            errors += 1;
            break;
        }
    }
    
    // Partial fill with 0xBB
    unsafe { core::ptr::write_bytes(buf.as_mut_ptr().add(100), 0xBB, 50); }
    if buf[99] != 0xAA || buf[100] != 0xBB || buf[149] != 0xBB || buf[150] != 0xAA {
        println!("Error: memory.fill partial failed");
        errors += 1;
    }
    
    errors
}

fn test_memory_copy() -> i32 {
    let mut errors = 0;
    
    // Non-overlapping copy (lowers to memory.copy / memcpy)
    let mut buf = [0u8; 100];
    for i in 0..50 { buf[i] = i as u8; }
    unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), buf.as_mut_ptr().add(50), 50); }
    for i in 0..50 {
        if buf[i+50] != i as u8 {
            println!("Error: memory.copy (non-overlapping) failed at index {}", i+50);
            errors += 1;
            break;
        }
    }
    
    // Overlapping forward (dest > src)
    let mut buf2 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    // Copy 0..5 to 2..7 -> [0, 1, 0, 1, 2, 3, 4, 7, 8, 9]
    unsafe { core::ptr::copy(buf2.as_ptr(), buf2.as_mut_ptr().add(2), 5); }
    let expected_f = [0, 1, 0, 1, 2, 3, 4, 7, 8, 9];
    if buf2 != expected_f {
        println!("Error: memory.copy (overlapping forward) failed: got {:?}", buf2);
        errors += 1;
    }
    
    // Overlapping backward (dest < src)
    let mut buf3 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    // Copy 2..7 to 0..5 -> [2, 3, 4, 5, 6, 5, 6, 7, 8, 9]
    unsafe { core::ptr::copy(buf3.as_ptr().add(2), buf3.as_mut_ptr(), 5); }
    let expected_b = [2, 3, 4, 5, 6, 5, 6, 7, 8, 9];
    if buf3 != expected_b {
        println!("Error: memory.copy (overlapping backward) failed: got {:?}", buf3);
        errors += 1;
    }

    errors
}

#[cfg(target_arch = "wasm32")]
fn test_memory_management() -> i32 {
    let mut errors = 0;
    let old_pages = core::arch::wasm32::memory_size(0);
    
    // memory.grow
    let prev_pages = core::arch::wasm32::memory_grow(0, 1);
    if prev_pages == usize::MAX {
        println!("Warning: memory.grow(1) failed (might be at memory limit)");
    } else {
        let new_pages = core::arch::wasm32::memory_size(0);
        if new_pages != old_pages + 1 {
            println!("Error: memory.size did not reflect growth: expected {}, got {}", old_pages + 1, new_pages);
            errors += 1;
        }
        
        // Verify that newly grown memory is zero-initialized per spec
        // Each page is 64KiB
        let offset = prev_pages * 65536;
        unsafe {
            let ptr = offset as *const u8;
            if *ptr != 0 {
                println!("Error: memory.grow memory not zero-initialized");
                errors += 1;
            }
        }
    }
    
    errors
}

#[cfg(not(target_arch = "wasm32"))]
fn test_memory_management() -> i32 { 0 }
