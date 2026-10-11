use core::arch::asm;

pub struct SbiRet {
    pub error: usize,
    pub value: usize,
}

#[inline(always)]
fn sbi_call(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize) -> SbiRet {
    sbi_call4(eid, fid, arg0, arg1, arg2, 0)
}

#[inline(always)]
fn sbi_call4(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize, arg3: usize) -> SbiRet {
    let mut error;
    let mut value;
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            in("a0") arg0,
            in("a1") arg1,
            in("a2") arg2,
            in("a3") arg3,
            lateout("a0") error,
            lateout("a1") value,
        );
    }
    SbiRet { error, value }
}

// HSM: start hart at physical start (MMU off), a0 = hart id, a1 = opaque.
// Nonzero error: no such hart, or already running.
pub fn hart_start(hart: usize, start: usize, opaque: usize) -> SbiRet {
    sbi_call(0x48534D, 0, hart, start, opaque)
}

// RFENCE: flush the whole TLB on every hart.
pub fn remote_sfence_vma_all() {
    // hart_mask_base = -1 selects all harts; size = -1 flushes everything.
    sbi_call4(0x52464E43, 1, 0, usize::MAX, 0, usize::MAX);
}

pub fn set_timer(stime_value: u64) {
    // SBI TIME Extension: EID = 0x54494D45, FID = 0
    sbi_call(0x54494D45, 0, stime_value as usize, 0, 0);
}

pub fn shutdown() -> ! {
    // SBI System Reset Extension: EID = 0x53525354, FID = 0
    sbi_call(0x53525354, 0, 0, 0, 0);
    loop {}
}
