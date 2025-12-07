ENTRY(_start)

SECTIONS {
    . = 0x00100000;

    KERNEL_START = .;

    .start : {
        *(.start)
    }

    .text : {
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
    }

    .eh_frame : {
        *(.eh_frame .eh_frame.*)
    }
    .eh_frame_hdr : {
        *(.eh_frame_hdr .eh_frame_hdr.*)
    }

    . = KERNEL_START + 0x00100000 - 2;

    .phony : {
        SHORT(0xFFFF);
    }
}