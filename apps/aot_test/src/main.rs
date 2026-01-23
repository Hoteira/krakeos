#![no_std]

// This function should be AOT compiled as it uses only supported instructions
use core::format_args;

#[inline(never)]
fn arithmetic_test(a: i32, b: i32) -> i32 {
    let add = a + b;
    let sub = a - b;
    let mul = a * b;
    let div = a / b;
    let rem = a % b;
    let logic = (add & sub) | (mul ^ div);
    let shift = (logic << 1) >> 1;
    shift + rem
}

#[inline(never)]
fn loop_test(n: i32) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    loop {
        if i >= n {
            break;
        }
        sum = sum + i;
        i = i + 1;
    }
    sum
}

#[inline(never)]
fn memory_test() -> i32 {
    // Allocate a small buffer on the stack (linear memory)
    let mut buf = [0u8; 4];
    buf[0] = 10;
    buf[1] = 20;
    
    // Reads from memory (stack)
    // The AOT compiler should handle the load instruction generated for array access.
    // The addition will use the loaded values.
    let val = buf[0] as i32 + buf[1] as i32;
    val
}


pub fn main() {
    let res = arithmetic_test(20, 10);
    // add=30, sub=10, mul=200, div=2, rem=0
    // logic = (30 & 10) | (200 ^ 2) = 10 | 202 = 202 (11001010)
    // shift = (202 << 1) >> 1 = 202
    // res = 202 + 0 = 202
    
    std::debugln!("Arithmetic Result: {}", res);
    
    let loop_res = loop_test(10); // sum 0..9 = 45
    std::debugln!("Loop Result: {}", loop_res);

    let mem_res = memory_test();
    std::debugln!("Memory Result: {}", mem_res);

    if res == 202 && loop_res == 45 && mem_res == 30 {
        std::debugln!("AOT Extended Test: SUCCESS");
    } else {
        std::debugln!("AOT Extended Test: FAILURE");
    }
}
