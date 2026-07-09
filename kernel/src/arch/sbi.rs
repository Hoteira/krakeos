use core::arch::asm;

pub struct SbiRet {
    pub error: usize,
    pub value: usize,
}

#[inline(always)]
fn sbi_call(eid: usize, fid: usize, arg0: usize, arg1: usize, arg2: usize) -> SbiRet {
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
            lateout("a0") error,
            lateout("a1") value,
        );
    }
    SbiRet { error, value }
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
