ENTRY(_blob_start)
SECTIONS {
    . = 0;
    .text : {
        KEEP(*(.jump_table))
        *(.text .text.*)
    }
    .rodata : { *(.rodata .rodata.*) }
    /DISCARD/ : { *(.eh_frame*) *(.note*) *(.comment) *(.gnu*) }
}
