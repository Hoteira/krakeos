#![no_std]
#![no_main]
#![feature(naked_functions)]

mod context;
mod syscall;
mod traps;
mod helpers;
mod simd;
mod memory;
mod globals;
mod tables;
mod store_ops;
mod wasi_fs;
mod wasi_proc;
mod wasi_env;
mod wasi_net;
mod wasi_p2;
mod float_helpers;
mod krakeos;

pub use context::Ring3Context;

#[no_mangle]
#[link_section = ".jump_table"]
pub static JUMP_TABLE: [unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128; 1024] = {
    let mut table: [unsafe extern "C" fn(&mut Ring3Context, *mut u128) -> *mut u128; 1024] = [traps::trap_generic; 1024];
    
    table[0] = traps::trap_generic;
    table[1] = traps::trap_oob;
    table[2] = traps::trap_fuel;
    table[3] = traps::trap_div_zero;
    table[4] = traps::trap_int_overflow;
    table[5] = traps::trap_indirect;
    table[6] = traps::trap_unreachable;
    table[7] = traps::trap_stack_overflow;
    table[8] = traps::trap_host;
    table[9] = traps::trap_unimplemented_fc;
    table[10] = traps::trap_unimplemented_simd;
    table[11] = traps::trap_unimplemented_atomic;
    
    table[12] = helpers::i32_div_s;
    table[13] = helpers::i32_div_u;
    table[14] = helpers::i32_rem_s;
    table[15] = helpers::i32_rem_u;
    table[16] = helpers::i64_div_s;
    table[17] = helpers::i64_div_u;
    table[18] = helpers::i64_rem_s;
    table[19] = helpers::i64_rem_u;

    // Float min/max helpers (C ABI: args in XMM0/XMM1, result in XMM0)
    table[32] = unsafe { core::mem::transmute(float_helpers::f32_min as *const ()) };  // F32Min = 32
    table[33] = unsafe { core::mem::transmute(float_helpers::f32_max as *const ()) };  // F32Max = 33
    table[34] = unsafe { core::mem::transmute(float_helpers::f64_min as *const ()) };  // F64Min = 34
    table[35] = unsafe { core::mem::transmute(float_helpers::f64_max as *const ()) };  // F64Max = 35

    // Saturating truncation helpers (C ABI: arg in XMM0, result in RAX)
    table[42] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_s as *const ()) };  // I32TruncSatF32S = 42
    table[43] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_u as *const ()) };  // I32TruncSatF32U = 43
    table[44] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_s as *const ()) };  // I32TruncSatF64S = 44
    table[45] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_u as *const ()) };  // I32TruncSatF64U = 45
    table[46] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_s as *const ()) };  // I64TruncSatF32S = 46
    table[47] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_u as *const ()) };  // I64TruncSatF32U = 47
    table[48] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f64_s as *const ()) };  // I64TruncSatF64S = 48
    table[49] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f64_u as *const ()) };  // I64TruncSatF64U = 49

    // Store-dependent bulk ops — forwarded to kernel via syscall (C ABI)
    table[50] = unsafe { core::mem::transmute(store_ops::ring3_memory_init as *const ()) };   // MemoryInit = 50
    table[51] = unsafe { core::mem::transmute(store_ops::ring3_data_drop as *const ()) };     // DataDrop = 51
    // Pure memory ops — handled directly in ring3 (C ABI)
    table[52] = unsafe { core::mem::transmute(memory::memory_copy as *const ()) };   // MemoryCopy = 52
    table[53] = unsafe { core::mem::transmute(memory::memory_fill as *const ()) };   // MemoryFill = 53
    // Table ops — forwarded to kernel via syscall (C ABI)
    table[54] = unsafe { core::mem::transmute(store_ops::ring3_table_init as *const ()) };    // TableInit = 54
    table[55] = unsafe { core::mem::transmute(store_ops::ring3_elem_drop as *const ()) };     // ElemDrop = 55
    table[56] = unsafe { core::mem::transmute(store_ops::ring3_table_copy as *const ()) };    // TableCopy = 56
    table[57] = unsafe { core::mem::transmute(store_ops::ring3_table_grow as *const ()) };    // TableGrow = 57
    table[58] = unsafe { core::mem::transmute(store_ops::ring3_table_size as *const ()) };    // TableSize = 58
    table[59] = unsafe { core::mem::transmute(store_ops::ring3_table_fill as *const ()) };    // TableFill = 59
    // sp-convention ops
    table[60] = memory::memory_size;   // MemorySize = 60
    table[61] = memory::memory_grow;   // MemoryGrow = 61

    // Call dispatch (must match AotTrampoline enum indices)
    table[66] = unsafe { core::mem::transmute(tables::call_indirect as *const ()) };  // CallIndirect = 66
    table[67] = unsafe { core::mem::transmute(wasi_fs::call_host_dispatch as *const ()) };  // CallHost = 67

    // WASI Preview 1 stubs (indices match import_stub_table in store/mod.rs)
    // Indices 300+ to avoid collision with AotTrampoline SIMD entries (84-260)
    table[300] = wasi_fs::wasi_fd_write;
    table[301] = wasi_fs::wasi_fd_read;
    table[302] = wasi_fs::wasi_fd_close;
    table[303] = wasi_fs::wasi_proc_exit;
    table[304] = wasi_fs::wasi_args_sizes_get;
    table[305] = wasi_fs::wasi_args_get;
    table[306] = wasi_fs::wasi_environ_sizes_get;
    table[307] = wasi_fs::wasi_environ_get;
    table[308] = wasi_fs::wasi_clock_time_get;
    table[309] = wasi_fs::wasi_random_get;
    table[310] = wasi_fs::wasi_fd_prestat_get;
    table[311] = wasi_fs::wasi_fd_prestat_dir_name;
    table[312] = wasi_fs::wasi_fd_fdstat_get;
    table[313] = wasi_fs::wasi_fd_filestat_get;
    table[314] = wasi_fs::wasi_fd_filestat_set_size;
    table[315] = wasi_fs::wasi_fd_seek;
    table[316] = wasi_fs::wasi_fd_pread;
    table[317] = wasi_fs::wasi_fd_readdir;
    table[318] = wasi_fs::wasi_path_open;
    table[319] = wasi_fs::wasi_path_filestat_get;
    table[320] = wasi_fs::wasi_path_create_directory;
    table[321] = wasi_fs::wasi_path_unlink_file;
    table[322] = wasi_fs::wasi_path_remove_directory;
    table[323] = wasi_fs::wasi_path_rename;
    table[324] = wasi_fs::wasi_path_link;
    table[325] = wasi_fs::wasi_path_symlink;
    table[326] = wasi_fs::wasi_path_readlink;
    table[327] = wasi_fs::wasi_poll_oneoff;
    table[328] = wasi_fs::wasi_sched_yield;
    table[329] = wasi_fs::wasi_clock_res_get;

    // KrakeOS Graphics (400+)
    table[400] = krakeos::krakeos_get_screen_width;
    table[401] = krakeos::krakeos_get_screen_height;

    // KrakeOS Window (410+)
    table[410] = krakeos::krakeos_window_create;
    table[411] = krakeos::krakeos_window_update;
    table[412] = krakeos::krakeos_window_update_area;
    table[413] = krakeos::krakeos_window_get_events;
    table[414] = krakeos::krakeos_register_event_queue;
    table[415] = krakeos::krakeos_deregister_event_queue;

    // KrakeOS Process (420+)
    table[420] = krakeos::krakeos_get_pid;
    table[421] = krakeos::krakeos_debug_print;
    table[422] = krakeos::krakeos_yield;
    table[423] = krakeos::krakeos_spawn;
    table[424] = krakeos::krakeos_waitpid;
    table[425] = krakeos::krakeos_pipe;
    table[426] = krakeos::krakeos_native_file_open;
    table[427] = krakeos::krakeos_native_file_stat;
    table[428] = krakeos::krakeos_file_read;
    table[429] = krakeos::krakeos_file_write;
    table[430] = krakeos::krakeos_kill;
    table[431] = krakeos::krakeos_get_list;
    table[432] = krakeos::krakeos_chdir;
    table[433] = krakeos::krakeos_get_slot_info;
    table[434] = krakeos::krakeos_ioctl;
    table[435] = krakeos::krakeos_set_nonblock;
    table[436] = krakeos::krakeos_poll;
    table[437] = krakeos::krakeos_get_current_user;
    table[438] = krakeos::krakeos_spawn_ext;
    table[439] = krakeos::krakeos_spawn_thread;
    table[440] = krakeos::krakeos_thread_exit;
    table[441] = krakeos::krakeos_syscall;

    // KrakeOS Memory (450+)
    table[450] = krakeos::krakeos_shm_get;
    table[451] = krakeos::krakeos_brk;
    table[452] = krakeos::krakeos_get_total_mem;
    table[453] = krakeos::krakeos_get_used_mem;
    table[454] = krakeos::krakeos_get_vma_dump;

    // Misc / compatibility
    table[460] = krakeos::krakeos_noop;
    table[461] = krakeos::krakeos_noop1;
    table[462] = krakeos::krakeos_noop2;

    // KrakeOS Terminal (463+)
    table[463] = krakeos::krakeos_terminal_set_window_size;
    table[464] = krakeos::krakeos_terminal_get_window_size;

    // KrakeOS Container (470+)
    table[470] = krakeos::krakeos_noop; // plant stub (TODO)
    table[471] = krakeos::krakeos_noop; // plant-from-path stub (TODO)
    table[472] = krakeos::krakeos_noop; // harvest stub (TODO)
    table[473] = krakeos::krakeos_noop; // list-children stub (TODO)
    table[474] = krakeos::krakeos_container_kill_child;

    // KrakeOS Debug (480+)
    table[480] = krakeos::krakeos_debug_get_process_list;
    table[481] = krakeos::krakeos_kill;
    table[482] = krakeos::krakeos_get_vma_dump;
    table[483] = krakeos::krakeos_noop; // get-memory-usage stub (TODO)

    // WASI Preview 2 (500+)
    table[500] = wasi_p2::wasi_p2_exit;
    table[501] = wasi_p2::wasi_p2_get_stdout;
    table[502] = wasi_p2::wasi_p2_get_stdin;
    table[503] = wasi_p2::wasi_p2_get_stderr;
    table[504] = wasi_p2::wasi_p2_output_stream_write;
    table[505] = wasi_p2::wasi_p2_input_stream_read;
    table[506] = wasi_p2::wasi_p2_poll;
    table[507] = wasi_p2::wasi_p2_pollable_block;
    table[508] = wasi_p2::wasi_p2_pollable_drop;
    table[509] = wasi_p2::wasi_p2_error_drop;
    table[510] = wasi_p2::wasi_p2_monotonic_now;
    table[511] = wasi_p2::wasi_p2_monotonic_resolution;
    table[512] = wasi_p2::wasi_p2_subscribe_duration;
    table[513] = wasi_p2::wasi_p2_wall_clock_now;
    table[514] = wasi_p2::wasi_p2_descriptor_drop;
    table[515] = wasi_p2::wasi_p2_descriptor_open_at;
    table[516] = wasi_p2::wasi_p2_descriptor_stat;
    table[517] = wasi_p2::wasi_p2_descriptor_set_size;
    table[518] = wasi_p2::wasi_p2_descriptor_seek;
    table[519] = wasi_p2::wasi_p2_descriptor_create_dir;
    table[520] = wasi_p2::wasi_p2_descriptor_unlink;
    table[521] = wasi_p2::wasi_p2_descriptor_rmdir;
    table[522] = wasi_p2::wasi_p2_descriptor_rename;
    table[523] = wasi_p2::wasi_p2_descriptor_read_directory;
    table[524] = wasi_p2::wasi_p2_dir_stream_drop;
    table[525] = wasi_p2::wasi_p2_get_random_bytes;
    table[526] = wasi_p2::wasi_p2_instance_network;

    table[999] = wasi_fs::wasi_serial_print;
    table[1023] = unsafe { core::mem::transmute(traps::process_exit as *const ()) };

    // SIMD operations (indices must match AotTrampoline enum in runtime.rs)
    table[69] = unsafe { core::mem::transmute(simd::v128_load_lane as *const ()) };     // V128LoadLane
    table[70] = unsafe { core::mem::transmute(simd::v128_store_lane as *const ()) };    // V128StoreLane
    // Bitwise
    table[71] = unsafe { core::mem::transmute(simd::v128_and as *const ()) };           // V128And
    table[72] = unsafe { core::mem::transmute(simd::v128_or as *const ()) };            // V128Or
    table[73] = unsafe { core::mem::transmute(simd::v128_xor as *const ()) };           // V128Xor
    table[74] = unsafe { core::mem::transmute(simd::v128_bitselect as *const ()) };     // V128Bitselect
    // V128Eq*
    table[75] = unsafe { core::mem::transmute(simd::v128_eq_i8x16 as *const ()) };     // V128EqI8x16
    table[76] = unsafe { core::mem::transmute(simd::v128_eq_i16x8 as *const ()) };     // V128EqI16x8
    table[77] = unsafe { core::mem::transmute(simd::v128_eq_i32x4 as *const ()) };     // V128EqI32x4
    table[78] = unsafe { core::mem::transmute(simd::v128_eq_i64x2 as *const ()) };     // V128EqI64x2
    table[79] = unsafe { core::mem::transmute(simd::v128_eq_f32x4 as *const ()) };     // V128EqF32x4
    table[80] = unsafe { core::mem::transmute(simd::v128_eq_f64x2 as *const ()) };     // V128EqF64x2
    // Reductions
    table[81] = unsafe { core::mem::transmute(simd::v128_any_true as *const ()) };      // V128AnyTrue
    table[82] = unsafe { core::mem::transmute(simd::v128_bitmask_i8x16 as *const ()) }; // V128BitmaskI8x16
    // Shuffle
    table[83] = unsafe { core::mem::transmute(simd::i8x16_shuffle as *const ()) };      // V128I8x16Shuffle
    // Integer arithmetic
    table[84] = unsafe { core::mem::transmute(simd::i8x16_add as *const ()) };          // I8x16Add
    table[85] = unsafe { core::mem::transmute(simd::i8x16_sub as *const ()) };          // I8x16Sub
    table[86] = unsafe { core::mem::transmute(simd::i16x8_add as *const ()) };          // I16x8Add
    table[87] = unsafe { core::mem::transmute(simd::i16x8_sub as *const ()) };          // I16x8Sub
    table[88] = unsafe { core::mem::transmute(simd::i16x8_mul as *const ()) };          // I16x8Mul
    table[89] = unsafe { core::mem::transmute(simd::i32x4_add as *const ()) };          // I32x4Add
    table[90] = unsafe { core::mem::transmute(simd::i32x4_sub as *const ()) };          // I32x4Sub
    table[91] = unsafe { core::mem::transmute(simd::i32x4_mul as *const ()) };          // I32x4Mul
    table[92] = unsafe { core::mem::transmute(simd::i64x2_add as *const ()) };          // I64x2Add
    table[93] = unsafe { core::mem::transmute(simd::i64x2_sub as *const ()) };          // I64x2Sub
    table[94] = unsafe { core::mem::transmute(simd::i64x2_mul as *const ()) };          // I64x2Mul
    // Float arithmetic
    table[95] = unsafe { core::mem::transmute(simd::f32x4_add as *const ()) };          // F32x4Add
    table[96] = unsafe { core::mem::transmute(simd::f32x4_sub as *const ()) };          // F32x4Sub
    table[97] = unsafe { core::mem::transmute(simd::f32x4_mul as *const ()) };          // F32x4Mul
    table[98] = unsafe { core::mem::transmute(simd::f32x4_div as *const ()) };          // F32x4Div
    table[99] = unsafe { core::mem::transmute(simd::f32x4_min as *const ()) };          // F32x4Min
    table[100] = unsafe { core::mem::transmute(simd::f32x4_max as *const ()) };         // F32x4Max
    table[101] = unsafe { core::mem::transmute(simd::f32x4_pmin as *const ()) };        // F32x4Pmin
    table[102] = unsafe { core::mem::transmute(simd::f32x4_pmax as *const ()) };        // F32x4Pmax
    table[103] = unsafe { core::mem::transmute(simd::f64x2_add as *const ()) };         // F64x2Add
    table[104] = unsafe { core::mem::transmute(simd::f64x2_sub as *const ()) };         // F64x2Sub
    table[105] = unsafe { core::mem::transmute(simd::f64x2_mul as *const ()) };         // F64x2Mul
    table[106] = unsafe { core::mem::transmute(simd::f64x2_div as *const ()) };         // F64x2Div
    table[107] = unsafe { core::mem::transmute(simd::f64x2_min as *const ()) };         // F64x2Min
    table[108] = unsafe { core::mem::transmute(simd::f64x2_max as *const ()) };         // F64x2Max
    table[109] = unsafe { core::mem::transmute(simd::f64x2_pmin as *const ()) };        // F64x2Pmin
    table[110] = unsafe { core::mem::transmute(simd::f64x2_pmax as *const ()) };        // F64x2Pmax
    // Integer relational
    table[111] = unsafe { core::mem::transmute(simd::i8x16_eq as *const ()) };          // I8x16Eq
    table[112] = unsafe { core::mem::transmute(simd::i8x16_ne as *const ()) };          // I8x16Ne
    table[113] = unsafe { core::mem::transmute(simd::i8x16_lt_s as *const ()) };        // I8x16LtS
    table[114] = unsafe { core::mem::transmute(simd::i8x16_lt_u as *const ()) };        // I8x16LtU
    table[115] = unsafe { core::mem::transmute(simd::i8x16_gt_s as *const ()) };        // I8x16GtS
    table[116] = unsafe { core::mem::transmute(simd::i8x16_gt_u as *const ()) };        // I8x16GtU
    table[117] = unsafe { core::mem::transmute(simd::i8x16_le_s as *const ()) };        // I8x16LeS
    table[118] = unsafe { core::mem::transmute(simd::i8x16_le_u as *const ()) };        // I8x16LeU
    table[119] = unsafe { core::mem::transmute(simd::i8x16_ge_s as *const ()) };        // I8x16GeS
    table[120] = unsafe { core::mem::transmute(simd::i8x16_ge_u as *const ()) };        // I8x16GeU
    table[121] = unsafe { core::mem::transmute(simd::i16x8_eq as *const ()) };          // I16x8Eq
    table[122] = unsafe { core::mem::transmute(simd::i16x8_ne as *const ()) };          // I16x8Ne
    table[123] = unsafe { core::mem::transmute(simd::i16x8_lt_s as *const ()) };        // I16x8LtS
    table[124] = unsafe { core::mem::transmute(simd::i16x8_lt_u as *const ()) };        // I16x8LtU
    table[125] = unsafe { core::mem::transmute(simd::i16x8_gt_s as *const ()) };        // I16x8GtS
    table[126] = unsafe { core::mem::transmute(simd::i16x8_gt_u as *const ()) };        // I16x8GtU
    table[127] = unsafe { core::mem::transmute(simd::i16x8_le_s as *const ()) };        // I16x8LeS
    table[128] = unsafe { core::mem::transmute(simd::i16x8_le_u as *const ()) };        // I16x8LeU
    table[129] = unsafe { core::mem::transmute(simd::i16x8_ge_s as *const ()) };        // I16x8GeS
    table[130] = unsafe { core::mem::transmute(simd::i16x8_ge_u as *const ()) };        // I16x8GeU
    table[131] = unsafe { core::mem::transmute(simd::i32x4_eq as *const ()) };          // I32x4Eq
    table[132] = unsafe { core::mem::transmute(simd::i32x4_ne as *const ()) };          // I32x4Ne
    table[133] = unsafe { core::mem::transmute(simd::i32x4_lt_s as *const ()) };        // I32x4LtS
    table[134] = unsafe { core::mem::transmute(simd::i32x4_lt_u as *const ()) };        // I32x4LtU
    table[135] = unsafe { core::mem::transmute(simd::i32x4_gt_s as *const ()) };        // I32x4GtS
    table[136] = unsafe { core::mem::transmute(simd::i32x4_gt_u as *const ()) };        // I32x4GtU
    table[137] = unsafe { core::mem::transmute(simd::i32x4_le_s as *const ()) };        // I32x4LeS
    table[138] = unsafe { core::mem::transmute(simd::i32x4_le_u as *const ()) };        // I32x4LeU
    table[139] = unsafe { core::mem::transmute(simd::i32x4_ge_s as *const ()) };        // I32x4GeS
    table[140] = unsafe { core::mem::transmute(simd::i32x4_ge_u as *const ()) };        // I32x4GeU
    table[141] = unsafe { core::mem::transmute(simd::i64x2_eq as *const ()) };          // I64x2Eq
    table[142] = unsafe { core::mem::transmute(simd::i64x2_ne as *const ()) };          // I64x2Ne
    table[143] = unsafe { core::mem::transmute(simd::i64x2_lt_s as *const ()) };        // I64x2LtS
    table[144] = unsafe { core::mem::transmute(simd::i64x2_gt_s as *const ()) };        // I64x2GtS
    table[145] = unsafe { core::mem::transmute(simd::i64x2_le_s as *const ()) };        // I64x2LeS
    table[146] = unsafe { core::mem::transmute(simd::i64x2_ge_s as *const ()) };        // I64x2GeS
    // Float relational
    table[147] = unsafe { core::mem::transmute(simd::f32x4_eq as *const ()) };          // F32x4Eq
    table[148] = unsafe { core::mem::transmute(simd::f32x4_ne as *const ()) };          // F32x4Ne
    table[149] = unsafe { core::mem::transmute(simd::f32x4_lt as *const ()) };          // F32x4Lt
    table[150] = unsafe { core::mem::transmute(simd::f32x4_gt as *const ()) };          // F32x4Gt
    table[151] = unsafe { core::mem::transmute(simd::f32x4_le as *const ()) };          // F32x4Le
    table[152] = unsafe { core::mem::transmute(simd::f32x4_ge as *const ()) };          // F32x4Ge
    table[153] = unsafe { core::mem::transmute(simd::f64x2_eq as *const ()) };          // F64x2Eq
    table[154] = unsafe { core::mem::transmute(simd::f64x2_ne as *const ()) };          // F64x2Ne
    table[155] = unsafe { core::mem::transmute(simd::f64x2_lt as *const ()) };          // F64x2Lt
    table[156] = unsafe { core::mem::transmute(simd::f64x2_gt as *const ()) };          // F64x2Gt
    table[157] = unsafe { core::mem::transmute(simd::f64x2_le as *const ()) };          // F64x2Le
    table[158] = unsafe { core::mem::transmute(simd::f64x2_ge as *const ()) };          // F64x2Ge
    // Integer unary
    table[159] = unsafe { core::mem::transmute(simd::i8x16_neg as *const ()) };         // I8x16Neg
    table[160] = unsafe { core::mem::transmute(simd::i8x16_abs as *const ()) };         // I8x16Abs
    table[161] = unsafe { core::mem::transmute(simd::i16x8_neg as *const ()) };         // I16x8Neg
    table[162] = unsafe { core::mem::transmute(simd::i16x8_abs as *const ()) };         // I16x8Abs
    table[163] = unsafe { core::mem::transmute(simd::i32x4_neg as *const ()) };         // I32x4Neg
    table[164] = unsafe { core::mem::transmute(simd::i32x4_abs as *const ()) };         // I32x4Abs
    table[165] = unsafe { core::mem::transmute(simd::i64x2_neg as *const ()) };         // I64x2Neg
    table[166] = unsafe { core::mem::transmute(simd::i64x2_abs as *const ()) };         // I64x2Abs
    // Float unary
    table[167] = unsafe { core::mem::transmute(simd::f32x4_neg as *const ()) };         // F32x4Neg
    table[168] = unsafe { core::mem::transmute(simd::f32x4_abs as *const ()) };         // F32x4Abs
    table[169] = unsafe { core::mem::transmute(simd::f32x4_sqrt as *const ()) };        // F32x4Sqrt
    table[170] = unsafe { core::mem::transmute(simd::f32x4_ceil as *const ()) };        // F32x4Ceil
    table[171] = unsafe { core::mem::transmute(simd::f32x4_floor as *const ()) };       // F32x4Floor
    table[172] = unsafe { core::mem::transmute(simd::f32x4_trunc as *const ()) };       // F32x4Trunc
    table[173] = unsafe { core::mem::transmute(simd::f32x4_nearest as *const ()) };     // F32x4Nearest
    table[174] = unsafe { core::mem::transmute(simd::f64x2_neg as *const ()) };         // F64x2Neg
    table[175] = unsafe { core::mem::transmute(simd::f64x2_abs as *const ()) };         // F64x2Abs
    table[176] = unsafe { core::mem::transmute(simd::f64x2_sqrt as *const ()) };        // F64x2Sqrt
    table[177] = unsafe { core::mem::transmute(simd::f64x2_ceil as *const ()) };        // F64x2Ceil
    table[178] = unsafe { core::mem::transmute(simd::f64x2_floor as *const ()) };       // F64x2Floor
    table[179] = unsafe { core::mem::transmute(simd::f64x2_trunc as *const ()) };       // F64x2Trunc
    table[180] = unsafe { core::mem::transmute(simd::f64x2_nearest as *const ()) };     // F64x2Nearest
    // V128Andnot
    table[181] = unsafe { core::mem::transmute(simd::v128_andnot as *const ()) };       // V128Andnot
    // Min/Max
    table[182] = unsafe { core::mem::transmute(simd::i8x16_min_s as *const ()) };      // I8x16MinS
    table[183] = unsafe { core::mem::transmute(simd::i8x16_min_u as *const ()) };      // I8x16MinU
    table[184] = unsafe { core::mem::transmute(simd::i8x16_max_s as *const ()) };      // I8x16MaxS
    table[185] = unsafe { core::mem::transmute(simd::i8x16_max_u as *const ()) };      // I8x16MaxU
    table[186] = unsafe { core::mem::transmute(simd::i16x8_min_s as *const ()) };      // I16x8MinS
    table[187] = unsafe { core::mem::transmute(simd::i16x8_min_u as *const ()) };      // I16x8MinU
    table[188] = unsafe { core::mem::transmute(simd::i16x8_max_s as *const ()) };      // I16x8MaxS
    table[189] = unsafe { core::mem::transmute(simd::i16x8_max_u as *const ()) };      // I16x8MaxU
    table[190] = unsafe { core::mem::transmute(simd::i32x4_min_s as *const ()) };      // I32x4MinS
    table[191] = unsafe { core::mem::transmute(simd::i32x4_min_u as *const ()) };      // I32x4MinU
    table[192] = unsafe { core::mem::transmute(simd::i32x4_max_s as *const ()) };      // I32x4MaxS
    table[193] = unsafe { core::mem::transmute(simd::i32x4_max_u as *const ()) };      // I32x4MaxU
    // Average
    table[194] = unsafe { core::mem::transmute(simd::i8x16_avgr_u as *const ()) };     // I8x16AvgrU
    table[195] = unsafe { core::mem::transmute(simd::i16x8_avgr_u as *const ()) };     // I16x8AvgrU
    // Saturating arithmetic
    table[196] = unsafe { core::mem::transmute(simd::i8x16_add_sat_s as *const ()) };  // I8x16AddSatS
    table[197] = unsafe { core::mem::transmute(simd::i8x16_add_sat_u as *const ()) };  // I8x16AddSatU
    table[198] = unsafe { core::mem::transmute(simd::i8x16_sub_sat_s as *const ()) };  // I8x16SubSatS
    table[199] = unsafe { core::mem::transmute(simd::i8x16_sub_sat_u as *const ()) };  // I8x16SubSatU
    table[200] = unsafe { core::mem::transmute(simd::i16x8_add_sat_s as *const ()) };  // I16x8AddSatS
    table[201] = unsafe { core::mem::transmute(simd::i16x8_add_sat_u as *const ()) };  // I16x8AddSatU
    table[202] = unsafe { core::mem::transmute(simd::i16x8_sub_sat_s as *const ()) };  // I16x8SubSatS
    table[203] = unsafe { core::mem::transmute(simd::i16x8_sub_sat_u as *const ()) };  // I16x8SubSatU
    // Popcnt
    table[204] = unsafe { core::mem::transmute(simd::i8x16_popcnt as *const ()) };     // I8x16Popcnt
    // More reductions
    table[205] = unsafe { core::mem::transmute(simd::v128_bitmask_i16x8 as *const ()) }; // V128BitmaskI16x8
    table[206] = unsafe { core::mem::transmute(simd::v128_bitmask_i32x4 as *const ()) }; // V128BitmaskI32x4
    table[207] = unsafe { core::mem::transmute(simd::v128_bitmask_i64x2 as *const ()) }; // V128BitmaskI64x2
    table[208] = unsafe { core::mem::transmute(simd::v128_all_true_i8x16 as *const ()) }; // V128AllTrueI8x16
    table[209] = unsafe { core::mem::transmute(simd::v128_all_true_i16x8 as *const ()) }; // V128AllTrueI16x8
    table[210] = unsafe { core::mem::transmute(simd::v128_all_true_i32x4 as *const ()) }; // V128AllTrueI32x4
    table[211] = unsafe { core::mem::transmute(simd::v128_all_true_i64x2 as *const ()) }; // V128AllTrueI64x2
    // Narrowing
    table[212] = unsafe { core::mem::transmute(simd::i8x16_narrow_i16x8_s as *const ()) }; // I8x16NarrowI16x8S
    table[213] = unsafe { core::mem::transmute(simd::i8x16_narrow_i16x8_u as *const ()) }; // I8x16NarrowI16x8U
    table[214] = unsafe { core::mem::transmute(simd::i16x8_narrow_i32x4_s as *const ()) }; // I16x8NarrowI32x4S
    table[215] = unsafe { core::mem::transmute(simd::i16x8_narrow_i32x4_u as *const ()) }; // I16x8NarrowI32x4U
    // Extend
    table[216] = unsafe { core::mem::transmute(simd::i16x8_extend_low_i8x16_s as *const ()) };  // I16x8ExtendLowI8x16S
    table[217] = unsafe { core::mem::transmute(simd::i16x8_extend_high_i8x16_s as *const ()) }; // I16x8ExtendHighI8x16S
    table[218] = unsafe { core::mem::transmute(simd::i16x8_extend_low_i8x16_u as *const ()) };  // I16x8ExtendLowI8x16U
    table[219] = unsafe { core::mem::transmute(simd::i16x8_extend_high_i8x16_u as *const ()) }; // I16x8ExtendHighI8x16U
    table[220] = unsafe { core::mem::transmute(simd::i32x4_extend_low_i16x8_s as *const ()) };  // I32x4ExtendLowI16x8S
    table[221] = unsafe { core::mem::transmute(simd::i32x4_extend_high_i16x8_s as *const ()) }; // I32x4ExtendHighI16x8S
    table[222] = unsafe { core::mem::transmute(simd::i32x4_extend_low_i16x8_u as *const ()) };  // I32x4ExtendLowI16x8U
    table[223] = unsafe { core::mem::transmute(simd::i32x4_extend_high_i16x8_u as *const ()) }; // I32x4ExtendHighI16x8U
    table[224] = unsafe { core::mem::transmute(simd::i64x2_extend_low_i32x4_s as *const ()) };  // I64x2ExtendLowI32x4S
    table[225] = unsafe { core::mem::transmute(simd::i64x2_extend_high_i32x4_s as *const ()) }; // I64x2ExtendHighI32x4S
    table[226] = unsafe { core::mem::transmute(simd::i64x2_extend_low_i32x4_u as *const ()) };  // I64x2ExtendLowI32x4U
    table[227] = unsafe { core::mem::transmute(simd::i64x2_extend_high_i32x4_u as *const ()) }; // I64x2ExtendHighI32x4U
    // Extmul
    table[228] = unsafe { core::mem::transmute(simd::i16x8_extmul_low_i8x16_s as *const ()) };  // I16x8ExtmulLowI8x16S
    table[229] = unsafe { core::mem::transmute(simd::i16x8_extmul_high_i8x16_s as *const ()) }; // I16x8ExtmulHighI8x16S
    table[230] = unsafe { core::mem::transmute(simd::i16x8_extmul_low_i8x16_u as *const ()) };  // I16x8ExtmulLowI8x16U
    table[231] = unsafe { core::mem::transmute(simd::i16x8_extmul_high_i8x16_u as *const ()) }; // I16x8ExtmulHighI8x16U
    table[232] = unsafe { core::mem::transmute(simd::i32x4_extmul_low_i16x8_s as *const ()) };  // I32x4ExtmulLowI16x8S
    table[233] = unsafe { core::mem::transmute(simd::i32x4_extmul_high_i16x8_s as *const ()) }; // I32x4ExtmulHighI16x8S
    table[234] = unsafe { core::mem::transmute(simd::i32x4_extmul_low_i16x8_u as *const ()) };  // I32x4ExtmulLowI16x8U
    table[235] = unsafe { core::mem::transmute(simd::i32x4_extmul_high_i16x8_u as *const ()) }; // I32x4ExtmulHighI16x8U
    table[236] = unsafe { core::mem::transmute(simd::i64x2_extmul_low_i32x4_s as *const ()) };  // I64x2ExtmulLowI32x4S
    table[237] = unsafe { core::mem::transmute(simd::i64x2_extmul_high_i32x4_s as *const ()) }; // I64x2ExtmulHighI32x4S
    table[238] = unsafe { core::mem::transmute(simd::i64x2_extmul_low_i32x4_u as *const ()) };  // I64x2ExtmulLowI32x4U
    table[239] = unsafe { core::mem::transmute(simd::i64x2_extmul_high_i32x4_u as *const ()) }; // I64x2ExtmulHighI32x4U
    // Extadd pairwise
    table[240] = unsafe { core::mem::transmute(simd::i16x8_extadd_pairwise_i8x16_s as *const ()) }; // I16x8ExtaddPairwiseI8x16S
    table[241] = unsafe { core::mem::transmute(simd::i16x8_extadd_pairwise_i8x16_u as *const ()) }; // I16x8ExtaddPairwiseI8x16U
    table[242] = unsafe { core::mem::transmute(simd::i32x4_extadd_pairwise_i16x8_s as *const ()) }; // I32x4ExtaddPairwiseI16x8S
    table[243] = unsafe { core::mem::transmute(simd::i32x4_extadd_pairwise_i16x8_u as *const ()) }; // I32x4ExtaddPairwiseI16x8U
    // Dot / Q15
    table[244] = unsafe { core::mem::transmute(simd::i32x4_dot_i16x8_s as *const ()) };   // I32x4DotI16x8S
    table[245] = unsafe { core::mem::transmute(simd::i16x8_q15mulrsat_s as *const ()) };  // I16x8Q15mulrsatS
    // Conversions
    table[246] = unsafe { core::mem::transmute(simd::i32x4_trunc_sat_f32x4_s as *const ()) };  // I32x4TruncSatF32x4S
    table[247] = unsafe { core::mem::transmute(simd::i32x4_trunc_sat_f32x4_u as *const ()) };  // I32x4TruncSatF32x4U
    table[248] = unsafe { core::mem::transmute(simd::f32x4_convert_i32x4_s as *const ()) };    // F32x4ConvertI32x4S
    table[249] = unsafe { core::mem::transmute(simd::f32x4_convert_i32x4_u as *const ()) };    // F32x4ConvertI32x4U
    table[250] = unsafe { core::mem::transmute(simd::i32x4_trunc_sat_f64x2_s_zero as *const ()) }; // I32x4TruncSatF64x2SZero
    table[251] = unsafe { core::mem::transmute(simd::i32x4_trunc_sat_f64x2_u_zero as *const ()) }; // I32x4TruncSatF64x2UZero
    table[252] = unsafe { core::mem::transmute(simd::f64x2_convert_low_i32x4_s as *const ()) }; // F64x2ConvertLowI32x4S
    table[253] = unsafe { core::mem::transmute(simd::f64x2_convert_low_i32x4_u as *const ()) }; // F64x2ConvertLowI32x4U
    // V128Not
    table[254] = unsafe { core::mem::transmute(simd::v128_not as *const ()) };           // V128Not
    // Load extend
    table[255] = unsafe { core::mem::transmute(simd::v128_load8x8_s as *const ()) };    // V128Load8x8S
    table[256] = unsafe { core::mem::transmute(simd::v128_load8x8_u as *const ()) };    // V128Load8x8U
    table[257] = unsafe { core::mem::transmute(simd::v128_load16x4_s as *const ()) };   // V128Load16x4S
    table[258] = unsafe { core::mem::transmute(simd::v128_load16x4_u as *const ()) };   // V128Load16x4U
    table[259] = unsafe { core::mem::transmute(simd::v128_load32x2_s as *const ()) };   // V128Load32x2S
    table[260] = unsafe { core::mem::transmute(simd::v128_load32x2_u as *const ()) };   // V128Load32x2U

    table
};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { syscall::syscall1(60, 99); } // SYS_EXIT(99)
    loop {}
}

#[no_mangle]
pub extern "C" fn _blob_start() {
    // Placeholder entry point
}
