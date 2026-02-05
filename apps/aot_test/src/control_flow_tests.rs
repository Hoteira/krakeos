use std::println;

pub fn test_complex_control_flow() -> i32 {
    println!("==================== Testing Complex Control Flow...");
    let mut errors = 0;

    errors += test_br_table_torture();
    errors += test_nested_unwind_torture();
    errors += test_loop_with_multiple_exits();
    errors += test_deep_recursion_with_state();
    errors += test_complex_conditionals_unwinding();

    if errors == 0 { println!("==================== Complex Control Flow: OK"); }
    errors
}

/// Stress test for br_table (the switch instruction).
/// AOT compilers often struggle with the correct stack height when jumping to various labels.
#[inline(never)]
fn test_br_table_torture() -> i32 {
    let mut errors = 0;

    fn torture_switch(selector: i32) -> i32 {
        let mut result = 0;
        // This match mimics a WASM br_table with targets at different block depths.
        match selector {
            0 => { result += 1; }
            1 => { result += 10; }
            2 => { result += 100; return result; } // Early return
            3 => { 
                // Nested block jump
                for i in 0..5 {
                    if i == 2 { result += 1000; break; }
                    result += 1;
                }
            }
            _ => { result = -1; }
        }
        result
    }

    if torture_switch(0) != 1 { errors += 1; println!("Error: br_table(0)"); }
    if torture_switch(1) != 10 { errors += 1; println!("Error: br_table(1)"); }
    if torture_switch(2) != 100 { errors += 1; println!("Error: br_table(2)"); }
    if torture_switch(3) != 1002 { errors += 1; println!("Error: br_table(3) got {}", torture_switch(3)); }
    if torture_switch(99) != -1 { errors += 1; println!("Error: br_table(default)"); }

    errors
}

/// Tests unwinding from deep nested blocks.
/// In WASM, breaking out of N blocks must correctly adjust the stack pointer (RSP).
#[inline(never)]
fn test_nested_unwind_torture() -> i32 {
    let mut errors = 0;
    let mut result = 0;

    // Outer loop to repeat the test
    for i in 0..10 {
        // Pushing values "on stack" (via locals/expressions)
        let a = i + 1;
        'outer: loop {
            let b = a + 2;
            'middle: loop {
                let c = b + 3;
                'inner: loop {
                    let d = c + 4;
                    if d > 15 {
                        result += d;
                        break 'outer; // Jumping out of 3 levels
                    }
                    result += 1;
                    break 'inner;
                }
                result += 10;
                break 'middle;
            }
            result += 100;
            break 'outer;
        }
    }

    // Trace:
    // i=0: a=1, b=3, c=6, d=10. d <= 15. result += 1. break inner. result += 10. break middle. result += 100. break outer. total 111.
    // i=1: a=2, b=4, c=7, d=11. d <= 15. result += 1. ... total 222.
    // i=2: a=3, b=5, c=8, d=12. ... total 333.
    // i=3: a=4, b=6, c=9, d=13. ... total 444.
    // i=4: a=5, b=7, c=10, d=14. ... total 555.
    // i=5: a=6, b=8, c=11, d=15. ... total 666.
    // i=6: a=7, b=9, c=12, d=16. d > 15. result += 16. break outer. total 682.
    // i=7: a=8, b=10, c=13, d=17. result += 17. break outer. total 699.
    // i=8: a=9, b=11, c=14, d=18. result += 18. break outer. total 717.
    // i=9: a=10, b=12, c=15, d=19. result += 19. break outer. total 736.
    
    if result != 736 {
        errors += 1;
        println!("Error: test_nested_unwind_torture: expected 736, got {}", result);
    }

    errors
}

/// Tests multiple exits from a loop (break and return).
#[inline(never)]
fn test_loop_with_multiple_exits() -> i32 {
    let mut errors = 0;

    fn multi_exit(n: i32) -> i32 {
        let mut i = 0;
        loop {
            if i >= 100 { return 999; }
            if i == n { break; }
            if i == 50 { return 500; }
            i += 1;
        }
        i
    }

    if multi_exit(5) != 5 { errors += 1; println!("Error: multi_exit(5)"); }
    if multi_exit(50) != 500 { errors += 1; println!("Error: multi_exit(50)"); }
    if multi_exit(200) != 999 { errors += 1; println!("Error: multi_exit(200)"); }

    errors
}

/// Deep recursion with state mutation to test stack frame management and spill/fill.
#[inline(never)]
fn test_deep_recursion_with_state() -> i32 {
    let mut errors = 0;

    fn ackermann(m: i32, n: i32) -> i32 {
        if m == 0 { n + 1 }
        else if m > 0 && n == 0 { ackermann(m - 1, 1) }
        else { ackermann(m - 1, ackermann(m, n - 1)) }
    }

    let res = ackermann(3, 2); // Should be 29
    if res != 29 {
        errors += 1;
        println!("Error: ackermann(3, 2) expected 29, got {}", res);
    }

    errors
}

/// Nested conditionals with internal branches that push/pop differently.
#[inline(never)]
fn test_complex_conditionals_unwinding() -> i32 {
    let mut errors = 0;
    
    fn logic(x: i32, y: i32) -> i32 {
        if x > 0 {
            if y > 0 {
                if x == y { return 1; }
                else { return 2; }
            } else {
                return 3;
            }
        } else {
            if y > 0 {
                return 4;
            } else {
                return 5;
            }
        }
    }

    if logic(1, 1) != 1 { errors += 1; println!("Error: logic(1, 1)"); }
    if logic(1, 2) != 2 { errors += 1; println!("Error: logic(1, 2)"); }
    if logic(1, -1) != 3 { errors += 1; println!("Error: logic(1, -1)"); }
    if logic(-1, 1) != 4 { errors += 1; println!("Error: logic(-1, 1)"); }
    if logic(-1, -1) != 5 { errors += 1; println!("Error: logic(-1, -1)"); }

    errors
}
