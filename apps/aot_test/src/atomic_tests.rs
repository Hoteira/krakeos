use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

#[cfg(target_arch = "wasm32")]
#[target_feature(enable = "atomics")]
pub unsafe fn test_atomics() -> i32 {
    use std::println;
    println!("==================== Testing Atomics...");
    let mut errors = 0;

    static VAL: AtomicI32 = AtomicI32::new(0);

    // 1. Load/Store
    VAL.store(42, Ordering::SeqCst);
    if VAL.load(Ordering::SeqCst) != 42 { println!("Error: Atomic Store/Load"); errors += 1; }

    // 2. Add (Fetch Add)
    let old = VAL.fetch_add(10, Ordering::SeqCst); // 42 -> 52
    if old != 42 { println!("Error: Atomic FetchAdd result"); errors += 1; }
    if VAL.load(Ordering::SeqCst) != 52 { println!("Error: Atomic FetchAdd value"); errors += 1; }

    // 3. Sub (Fetch Sub)
    VAL.fetch_sub(2, Ordering::SeqCst); // 52 -> 50
    if VAL.load(Ordering::SeqCst) != 50 { println!("Error: Atomic FetchSub"); errors += 1; }

    // 4. And/Or/Xor
    VAL.store(0b1010, Ordering::SeqCst);
    VAL.fetch_or(0b0101, Ordering::SeqCst); // 1010 | 0101 = 1111 (15)
    if VAL.load(Ordering::SeqCst) != 15 { println!("Error: Atomic FetchOr"); errors += 1; }
    
    VAL.fetch_and(0b1011, Ordering::SeqCst); // 1111 & 1011 = 1011 (11)
    if VAL.load(Ordering::SeqCst) != 11 { println!("Error: Atomic FetchAnd"); errors += 1; }

    // 5. Compare Exchange
    // VAL is 11.
    // Try to swap 11 -> 100. Should succeed.
    let res = VAL.compare_exchange(11, 100, Ordering::SeqCst, Ordering::SeqCst);
    if res.is_err() || VAL.load(Ordering::SeqCst) != 100 { println!("Error: Atomic CompareExchange (Success)"); errors += 1; }
    
    // Try to swap 11 -> 200. Should fail (VAL is 100).
    let res = VAL.compare_exchange(11, 200, Ordering::SeqCst, Ordering::SeqCst);
    if res.is_ok() || VAL.load(Ordering::SeqCst) != 100 { println!("Error: Atomic CompareExchange (Failure)"); errors += 1; }

    /*
    // 5. Compare Exchange
    // VAL is 11.
    // Try to swap 11 -> 100. Should succeed.
    let res = VAL.compare_exchange(11, 100, Ordering::SeqCst, Ordering::SeqCst);
    if res.is_err() || VAL.load(Ordering::SeqCst) != 100 { println!("Error: Atomic CompareExchange (Success)"); errors += 1; }
    
    // Try to swap 11 -> 200. Should fail (VAL is 100).
    let res = VAL.compare_exchange(11, 200, Ordering::SeqCst, Ordering::SeqCst);
    if res.is_ok() || VAL.load(Ordering::SeqCst) != 100 { println!("Error: Atomic CompareExchange (Failure)"); errors += 1; }
    */

    if errors == 0 { println!("==================== Atomics: OK"); }
    errors
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn test_atomics() -> i32 {
    0
}
