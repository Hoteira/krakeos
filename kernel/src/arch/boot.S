.section .text.boot
.global _start
_start:
    bnez a0, halt
    la sp, __stack_top
    call main

halt:
    wfi
    j halt
