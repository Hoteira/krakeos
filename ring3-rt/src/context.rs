#[repr(C)]
pub struct Ring3Context {
    pub _reserved0: u64,             // +0   (was store pointer — unused in ring3)
    pub _reserved1: u64,             // +8   (was fuel pointer — unused in ring3)
    pub memory_base: *mut u8,        // +16  Linear memory base
    pub memory_size: usize,          // +24  Linear memory current size
    pub stack_base: *mut u128,       // +32  AOT execution stack base
    pub locals_base: *mut u128,      // +40  Local variables area
    pub _reserved2: usize,           // +48  (was module_addr — unused in ring3)
    pub stack_limit: usize,          // +56  Stack overflow boundary
    pub trap_code: *mut i32,         // +64  Trap code output
    pub blob_base: u64,              // +72  Base address of blob in code slot
    // --- New fields below (offsets > 72) ---
    pub globals_ptr: *mut u8,        // +80  Pointer to globals array in data region
    pub globals_count: u32,          // +88  Number of globals
    pub _pad0: u32,                  // +92
    pub table0_ptr: *mut u64,        // +96  Pointer to table 0 entries (code slot offsets)
    pub table0_size: u32,            // +104
    pub _pad1: u32,                  // +108
    pub func_table_ptr: *mut u64,    // +112 Code offsets for all functions
    pub func_count: u32,             // +120
    pub _pad2: u32,                  // +124
    pub pid: u64,                    // +128
    pub slot_id: u16,                // +136
    pub _pad3: [u8; 6],              // +138
    pub trap_code_storage: i32,      // +144 Actual trap code value
    pub _pad4: u32,                  // +148
    pub num_imported_funcs: u32,     // +152 Number of imported functions
    pub _pad5: u32,                  // +156
    pub import_stub_table: *const u64, // +160 Table mapping import index → blob stub offset
}
