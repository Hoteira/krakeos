ENTRY(_start)

SECTIONS {
    . = 0xFFFFFFFF00100000;

    .start : AT(ADDR(.start) - 0xFFFFFFFF00000000) {
        *(.start)
    }

    .text : AT(ADDR(.text) - 0xFFFFFFFF00000000) {
        *(.text .text.*)
    }

    .rodata : AT(ADDR(.rodata) - 0xFFFFFFFF00000000) {
        *(.rodata .rodata.*)
    }

    .data : AT(ADDR(.data) - 0xFFFFFFFF00000000) {
        *(.data .data.*)
    }

    .bss : AT(ADDR(.bss) - 0xFFFFFFFF00000000) {
        *(.bss .bss.*)
    }

    .eh_frame : AT(ADDR(.eh_frame) - 0xFFFFFFFF00000000) {
        *(.eh_frame .eh_frame.*)
    }
    .eh_frame_hdr : AT(ADDR(.eh_frame_hdr) - 0xFFFFFFFF00000000) {
        *(.eh_frame_hdr .eh_frame_hdr.*)
    }
}