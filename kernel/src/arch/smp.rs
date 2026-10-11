// Multi-hart support. The trap handler holds the big kernel lock, so only one
// hart runs kernel code at a time; tp holds the hart id inside the kernel.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::{csr, sbi};

pub const MAX_HARTS: usize = 8;
const SECONDARY_STACK: usize = 64 * 1024;

// The hart that booted the kernel; it alone polls input and draws.
pub static BOOT_HART: AtomicUsize = AtomicUsize::new(0);
pub static ONLINE: AtomicUsize = AtomicUsize::new(1);
// Set once secondaries start, so TLB flushes go to every hart.
pub static SMP: AtomicBool = AtomicBool::new(false);
static mut SATP: u64 = 0;

#[inline(always)]
pub fn hart() -> usize {
    let h: usize;
    unsafe { core::arch::asm!("mv {}, tp", out(reg) h) };
    h
}

pub fn is_boot_hart() -> bool {
    hart() == BOOT_HART.load(Ordering::Relaxed)
}

// Ticket lock, so a hart spinning in a trap can't be starved.
static NEXT_TICKET: AtomicUsize = AtomicUsize::new(0);
static NOW_SERVING: AtomicUsize = AtomicUsize::new(0);
// Hart id + 1 of the holder (0 = free), for the re-entry check.
static OWNER: AtomicUsize = AtomicUsize::new(0);
static mut DEPTH: usize = 0;

// Re-entrant on the same hart, so a fault inside the kernel can still print.
pub fn lock() {
    let me = hart() + 1;
    if OWNER.load(Ordering::Relaxed) == me {
        unsafe { DEPTH += 1 };
        return;
    }
    let ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
    while NOW_SERVING.load(Ordering::Acquire) != ticket {
        core::hint::spin_loop();
    }
    OWNER.store(me, Ordering::Relaxed);
}

pub fn unlock() {
    unsafe {
        if DEPTH > 0 {
            DEPTH -= 1;
            return;
        }
    }
    OWNER.store(0, Ordering::Relaxed);
    NOW_SERVING.fetch_add(1, Ordering::Release);
}

// Called by the boot hart once the kernel is set up, with the lock free.
pub fn start_others(satp: u64) {
    unsafe extern "C" {
        fn _secondary_start();
    }
    unsafe { SATP = satp };
    SMP.store(true, Ordering::SeqCst);
    let me = hart();
    let layout = core::alloc::Layout::from_size_align(SECONDARY_STACK, 4096).unwrap();
    for h in 0..MAX_HARTS {
        if h == me {
            continue;
        }
        let stack = unsafe { alloc::alloc::alloc_zeroed(layout) } as usize;
        if stack == 0 {
            break;
        }
        let r = sbi::hart_start(h, _secondary_start as *const () as usize, stack + SECONDARY_STACK);
        if r.error != 0 {
            unsafe { alloc::alloc::dealloc(stack as *mut u8, layout) };
        }
    }
}

// Secondary hart entry from boot.S: share the boot hart's page table and trap vector.
#[no_mangle]
pub extern "C" fn secondary_main(_hart_id: usize) -> ! {
    unsafe {
        core::arch::asm!("csrw stvec, {}", in(reg) crate::trap::trap_vector as *const () as usize);
        csr::write_satp(SATP);
        csr::sfence_vma_local();
        core::arch::asm!("csrs sstatus, {}", in(reg) 1 << 13);
    }
    lock();
    let n = ONLINE.fetch_add(1, Ordering::SeqCst) + 1;
    crate::println!("hart {} online ({} running)", hart(), n);
    unlock();
    run_idle()
}

// The scheduler takes over on the first timer tick.
pub fn run_idle() -> ! {
    csr::enable_timer_interrupt();
    csr::enable_global_interrupts();
    sbi::set_timer(csr::read_time() + 10_000);
    loop {
        unsafe { core::arch::asm!("wfi") };
    }
}
