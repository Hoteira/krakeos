.section .text.boot
.global _start
_start:
    # Whichever hart OpenSBI picked boots the kernel; it starts the others.
    mv tp, a0
    la sp, __stack_top
    call main

halt:
    wfi
    j halt

# Secondary harts, started through SBI HSM: a0 = hart id, a1 = stack top.
.global _secondary_start
_secondary_start:
    mv tp, a0
    mv sp, a1
    call secondary_main
    j halt
