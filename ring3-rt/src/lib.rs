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

    // FP Relops
    table[20] = unsafe { core::mem::transmute(float_helpers::f32_eq as *const ()) };
    table[21] = unsafe { core::mem::transmute(float_helpers::f32_ne as *const ()) };
    table[22] = unsafe { core::mem::transmute(float_helpers::f32_lt as *const ()) };
    table[23] = unsafe { core::mem::transmute(float_helpers::f32_gt as *const ()) };
    table[24] = unsafe { core::mem::transmute(float_helpers::f32_le as *const ()) };
    table[25] = unsafe { core::mem::transmute(float_helpers::f32_ge as *const ()) };
    table[26] = unsafe { core::mem::transmute(float_helpers::f64_eq as *const ()) };
    table[27] = unsafe { core::mem::transmute(float_helpers::f64_ne as *const ()) };
    table[28] = unsafe { core::mem::transmute(float_helpers::f64_lt as *const ()) };
    table[29] = unsafe { core::mem::transmute(float_helpers::f64_gt as *const ()) };
    table[30] = unsafe { core::mem::transmute(float_helpers::f64_le as *const ()) };
    table[31] = unsafe { core::mem::transmute(float_helpers::f64_ge as *const ()) };

    // Float min/max helpers
    table[32] = unsafe { core::mem::transmute(float_helpers::f32_min as *const ()) };  // F32Min = 32
    table[33] = unsafe { core::mem::transmute(float_helpers::f32_max as *const ()) };  // F32Max = 33
    table[34] = unsafe { core::mem::transmute(float_helpers::f64_min as *const ()) };  // F64Min = 34
    table[35] = unsafe { core::mem::transmute(float_helpers::f64_max as *const ()) };  // F64Max = 35

    // Conversions
    table[36] = unsafe { core::mem::transmute(float_helpers::f32_convert_i64_u as *const ()) };
    table[37] = unsafe { core::mem::transmute(float_helpers::f64_convert_i64_u as *const ()) };
    table[38] = unsafe { core::mem::transmute(float_helpers::i32_trunc_f32_u as *const ()) };
    table[39] = unsafe { core::mem::transmute(float_helpers::i32_trunc_f64_u as *const ()) };
    table[40] = unsafe { core::mem::transmute(float_helpers::i64_trunc_f32_u as *const ()) };
    table[41] = unsafe { core::mem::transmute(float_helpers::i64_trunc_f64_u as *const ()) };

    // Saturating truncation helpers
    table[42] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_s as *const ()) };  // I32TruncSatF32S = 42
    table[43] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f32_u as *const ()) };  // I32TruncSatF32U = 43
    table[44] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_s as *const ()) };  // I32TruncSatF64S = 44
    table[45] = unsafe { core::mem::transmute(float_helpers::i32_trunc_sat_f64_u as *const ()) };  // I32TruncSatF64U = 45
    table[46] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_s as *const ()) };  // I64TruncSatF32S = 46
    table[47] = unsafe { core::mem::transmute(float_helpers::i64_trunc_sat_f32_u as *const ()) };  // I32TruncSatF32U = 47
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

    // Globals/Tables
    table[62] = unsafe { core::mem::transmute(globals::global_get as *const ()) }; // GlobalGet = 62
    table[63] = unsafe { core::mem::transmute(globals::global_set as *const ()) }; // GlobalSet = 63
    table[64] = unsafe { core::mem::transmute(tables::table_get as *const ()) };  // TableGet = 64
    table[65] = unsafe { core::mem::transmute(tables::table_set as *const ()) };  // TableSet = 65

    // Call dispatch (must match AotTrampoline enum indices)
    table[66] = unsafe { core::mem::transmute(tables::call_indirect as *const ()) };  // CallIndirect = 66
    table[67] = unsafe { core::mem::transmute(wasi_fs::call_host_dispatch as *const ()) };  // CallHost = 67
    table[68] = unsafe { core::mem::transmute(trampoline_ref_func as *const ()) }; // RefFunc = 68

    // FP Rounding
    table[69] = unsafe { core::mem::transmute(float_helpers::f32_ceil as *const ()) }; // F32Ceil = 69
    table[70] = unsafe { core::mem::transmute(float_helpers::f32_floor as *const ()) }; // F32Floor = 70
    table[71] = unsafe { core::mem::transmute(float_helpers::f32_trunc as *const ()) }; // F32Trunc = 71
    table[72] = unsafe { core::mem::transmute(float_helpers::f32_nearest as *const ()) }; // F32Nearest = 72
    table[73] = unsafe { core::mem::transmute(float_helpers::f64_ceil as *const ()) }; // F64Ceil = 73
    table[74] = unsafe { core::mem::transmute(float_helpers::f64_floor as *const ()) }; // F64Floor = 74
    table[75] = unsafe { core::mem::transmute(float_helpers::f64_trunc as *const ()) }; // F64Trunc = 75
    table[76] = unsafe { core::mem::transmute(float_helpers::f64_nearest as *const ()) }; // F64Nearest = 76

    // SIMD operations (indices must match AotTrampoline enum in runtime.rs)
    table[77] = unsafe { core::mem::transmute(simd::v128_load_lane as *const ()) };     // V128LoadLane = 77
    table[78] = unsafe { core::mem::transmute(simd::v128_store_lane as *const ()) };    // V128StoreLane = 78
    // Bitwise
    table[79] = unsafe { core::mem::transmute(simd::v128_and as *const ()) };           // V128And = 79
    table[80] = unsafe { core::mem::transmute(simd::v128_or as *const ()) };            // V128Or = 80
    table[81] = unsafe { core::mem::transmute(simd::v128_xor as *const ()) };           // V128Xor = 81
    table[82] = unsafe { core::mem::transmute(simd::v128_bitselect as *const ()) };     // V128Bitselect = 82
    // V128Eq*
    table[83] = unsafe { core::mem::transmute(simd::v128_eq_i8x16 as *const ()) };     // V128EqI8x16 = 83
    table[84] = unsafe { core::mem::transmute(simd::v128_eq_i16x8 as *const ()) };     // V128EqI16x8 = 84
    table[85] = unsafe { core::mem::transmute(simd::v128_eq_i32x4 as *const ()) };     // V128EqI32x4 = 85
    table[86] = unsafe { core::mem::transmute(simd::v128_eq_i64x2 as *const ()) };     // V128EqI64x2 = 86
    table[87] = unsafe { core::mem::transmute(simd::v128_eq_f32x4 as *const ()) };     // V128EqF32x4 = 87
    table[88] = unsafe { core::mem::transmute(simd::v128_eq_f64x2 as *const ()) };     // V128EqF64x2 = 88
    // Reductions
    table[89] = unsafe { core::mem::transmute(simd::v128_any_true as *const ()) };      // V128AnyTrue = 89
    table[90] = unsafe { core::mem::transmute(simd::v128_bitmask_i8x16 as *const ()) }; // V128BitmaskI8x16 = 90
    // Shuffle
    table[91] = unsafe { core::mem::transmute(simd::i8x16_shuffle as *const ()) };      // V128I8x16Shuffle = 91
    // Integer arithmetic
    table[92] = unsafe { core::mem::transmute(simd::i8x16_add as *const ()) };          // I8x16Add = 92
    table[93] = unsafe { core::mem::transmute(simd::i8x16_sub as *const ()) };          // I8x16Sub = 93
    table[94] = unsafe { core::mem::transmute(simd::i16x8_add as *const ()) };          // I16x8Add = 94
    table[95] = unsafe { core::mem::transmute(simd::i16x8_sub as *const ()) };          // I16x8Sub = 95
    table[96] = unsafe { core::mem::transmute(simd::i16x8_mul as *const ()) };          // I16x8Mul = 96
    table[97] = unsafe { core::mem::transmute(simd::i32x4_add as *const ()) };          // I32x4Add = 97
    table[98] = unsafe { core::mem::transmute(simd::i32x4_sub as *const ()) };          // I32x4Sub = 98
    table[99] = unsafe { core::mem::transmute(simd::i32x4_mul as *const ()) };          // I32x4Mul = 99
    table[100] = unsafe { core::mem::transmute(simd::i64x2_add as *const ()) };          // I64x2Add = 100
    table[101] = unsafe { core::mem::transmute(simd::i64x2_sub as *const ()) };          // I64x2Sub = 101
    table[102] = unsafe { core::mem::transmute(simd::i64x2_mul as *const ()) };          // I64x2Mul = 102
    // Float arithmetic
    table[103] = unsafe { core::mem::transmute(simd::f32x4_add as *const ()) };          // F32x4Add = 103
    table[104] = unsafe { core::mem::transmute(simd::f32x4_sub as *const ()) };          // F32x4Sub = 104
    table[105] = unsafe { core::mem::transmute(simd::f32x4_mul as *const ()) };          // F32x4Mul = 105
    table[106] = unsafe { core::mem::transmute(simd::f32x4_div as *const ()) };          // F32x4Div = 106
    table[107] = unsafe { core::mem::transmute(simd::f32x4_min as *const ()) };          // F32x4Min = 107
    table[108] = unsafe { core::mem::transmute(simd::f32x4_max as *const ()) };         // F32x4Max = 108
    table[109] = unsafe { core::mem::transmute(simd::f32x4_pmin as *const ()) };        // F32x4Pmin = 109
    table[110] = unsafe { core::mem::transmute(simd::f32x4_pmax as *const ()) };        // F32x4Pmax = 110
    table[111] = unsafe { core::mem::transmute(simd::f64x2_add as *const ()) };         // F64x2Add = 111
    table[112] = unsafe { core::mem::transmute(simd::f64x2_sub as *const ()) };         // F64x2Sub = 112
    table[113] = unsafe { core::mem::transmute(simd::f64x2_mul as *const ()) };         // F64x2Mul = 113
    table[114] = unsafe { core::mem::transmute(simd::f64x2_div as *const ()) };         // F64x2Div = 114
    table[115] = unsafe { core::mem::transmute(simd::f64x2_min as *const ()) };         // F64x2Min = 115
    table[116] = unsafe { core::mem::transmute(simd::f64x2_max as *const ()) };         // F64x2Max = 116
    table[117] = unsafe { core::mem::transmute(simd::f64x2_pmin as *const ()) };        // F64x2Pmin = 117
    table[118] = unsafe { core::mem::transmute(simd::f64x2_pmax as *const ()) };        // F64x2Pmax = 118
    // Integer relational
    table[119] = unsafe { core::mem::transmute(simd::i8x16_eq as *const ()) };          // I8x16Eq = 119
    table[120] = unsafe { core::mem::transmute(simd::i8x16_ne as *const ()) };          // I8x16Ne = 120
    table[121] = unsafe { core::mem::transmute(simd::i8x16_lt_s as *const ()) };        // I8x16LtS = 121
    table[122] = unsafe { core::mem::transmute(simd::i8x16_lt_u as *const ()) };        // I8x16LtU = 122
    table[123] = unsafe { core::mem::transmute(simd::i8x16_gt_s as *const ()) };        // I8x16GtS = 123
    table[124] = unsafe { core::mem::transmute(simd::i8x16_gt_u as *const ()) };        // I8x16GtU = 124
    table[125] = unsafe { core::mem::transmute(simd::i8x16_le_s as *const ()) };        // I8x16LeS = 125
    table[126] = unsafe { core::mem::transmute(simd::i8x16_le_u as *const ()) };        // I8x16LeU = 126
    table[127] = unsafe { core::mem::transmute(simd::i8x16_ge_s as *const ()) };        // I8x16GeS = 127
    table[128] = unsafe { core::mem::transmute(simd::i8x16_ge_u as *const ()) };        // I8x16GeU = 128
    table[129] = unsafe { core::mem::transmute(simd::i16x8_eq as *const ()) };          // I16x8Eq = 129
    table[130] = unsafe { core::mem::transmute(simd::i16x8_ne as *const ()) };          // I16x8Ne = 130
    table[131] = unsafe { core::mem::transmute(simd::i16x8_lt_s as *const ()) };        // I16x8LtS = 131
    table[132] = unsafe { core::mem::transmute(simd::i16x8_lt_u as *const ()) };        // I16x8LtU = 132
    table[133] = unsafe { core::mem::transmute(simd::i16x8_gt_s as *const ()) };        // I16x8GtS = 133
    table[134] = unsafe { core::mem::transmute(simd::i16x8_gt_u as *const ()) };        // I16x8GtU = 134
    table[135] = unsafe { core::mem::transmute(simd::i16x8_le_s as *const ()) };        // I16x8LeS = 135
    table[136] = unsafe { core::mem::transmute(simd::i16x8_le_u as *const ()) };        // I16x8LeU = 136
    table[137] = unsafe { core::mem::transmute(simd::i16x8_ge_s as *const ()) };        // I16x8GeS = 137
    table[138] = unsafe { core::mem::transmute(simd::i16x8_ge_u as *const ()) };        // I16x8GeU = 138
    table[139] = unsafe { core::mem::transmute(simd::i32x4_eq as *const ()) };          // I32x4Eq = 139
    table[140] = unsafe { core::mem::transmute(simd::i32x4_ne as *const ()) };          // I32x4Ne = 140
    table[141] = unsafe { core::mem::transmute(simd::i32x4_lt_s as *const ()) };        // I32x4LtS = 141
    table[142] = unsafe { core::mem::transmute(simd::i32x4_lt_u as *const ()) };        // I32x4LtU = 142
    table[143] = unsafe { core::mem::transmute(simd::i32x4_gt_s as *const ()) };        // I32x4GtS = 143
    table[144] = unsafe { core::mem::transmute(simd::i32x4_gt_u as *const ()) };        // I32x4GtU = 144
    table[145] = unsafe { core::mem::transmute(simd::i32x4_le_s as *const ()) };        // I32x4LeS = 145
    table[146] = unsafe { core::mem::transmute(simd::i32x4_le_u as *const ()) };        // I32x4LeU = 146
    table[147] = unsafe { core::mem::transmute(simd::i32x4_ge_s as *const ()) };        // I32x4GeS = 147
    table[148] = unsafe { core::mem::transmute(simd::i32x4_ge_u as *const ()) };        // I32x4GeU = 148
    table[149] = unsafe { core::mem::transmute(simd::i64x2_eq as *const ()) };          // I64x2Eq = 149
    table[150] = unsafe { core::mem::transmute(simd::i64x2_ne as *const ()) };          // I64x2Ne = 150
    table[151] = unsafe { core::mem::transmute(simd::i64x2_lt_s as *const ()) };        // I64x2LtS = 151
    table[152] = unsafe { core::mem::transmute(simd::i64x2_gt_s as *const ()) };        // I64x2GtS = 152
    table[153] = unsafe { core::mem::transmute(simd::i64x2_le_s as *const ()) };        // I64x2LeS = 153
    table[154] = unsafe { core::mem::transmute(simd::i64x2_ge_s as *const ()) };        // I64x2GeS = 154
    // Float relational
    table[155] = unsafe { core::mem::transmute(simd::f32x4_eq as *const ()) };          // F32x4Eq = 155
    table[156] = unsafe { core::mem::transmute(simd::f32x4_ne as *const ()) };          // F32x4Ne = 156
    table[157] = unsafe { core::mem::transmute(simd::f32x4_lt as *const ()) };          // F32x4Lt = 157
    table[158] = unsafe { core::mem::transmute(simd::f32x4_gt as *const ()) };          // F32x4Gt = 158
    table[159] = unsafe { core::mem::transmute(simd::f32x4_le as *const ()) };          // F32x4Le = 159
    table[160] = unsafe { core::mem::transmute(simd::f32x4_ge as *const ()) };          // F32x4Ge = 160
    table[161] = unsafe { core::mem::transmute(simd::f64x2_eq as *const ()) };          // F64x2Eq = 161
    table[162] = unsafe { core::mem::transmute(simd::f64x2_ne as *const ()) };          // F64x2Ne = 162
    table[163] = unsafe { core::mem::transmute(simd::f64x2_lt as *const ()) };          // F64x2Lt = 163
    table[164] = unsafe { core::mem::transmute(simd::f64x2_gt as *const ()) };          // F64x2Gt = 164
    table[165] = unsafe { core::mem::transmute(simd::f64x2_le as *const ()) };          // F64x2Le = 165
    table[166] = unsafe { core::mem::transmute(simd::f64x2_ge as *const ()) };          // F64x2Ge = 166
    // Integer unary
    table[167] = unsafe { core::mem::transmute(simd::i8x16_neg as *const ()) };         // I8x16Neg = 167
    table[168] = unsafe { core::mem::transmute(simd::i8x16_abs as *const ()) };         // I8x16Abs = 168
    table[169] = unsafe { core::mem::transmute(simd::i16x8_neg as *const ()) };         // I16x8Neg = 169
    table[170] = unsafe { core::mem::transmute(simd::i16x8_abs as *const ()) };         // I16x8Abs = 170
    table[171] = unsafe { core::mem::transmute(simd::i32x4_neg as *const ()) };         // I32x4Neg = 171
    table[172] = unsafe { core::mem::transmute(simd::i32x4_abs as *const ()) };         // I32x4Abs = 172
    table[173] = unsafe { core::mem::transmute(simd::i64x2_neg as *const ()) };         // I64x2Neg = 173
    table[174] = unsafe { core::mem::transmute(simd::i64x2_abs as *const ()) };         // I64x2Abs = 174
    // Float unary
    table[175] = unsafe { core::mem::transmute(simd::f32x4_neg as *const ()) };         // F32x4Neg = 175
    table[176] = unsafe { core::mem::transmute(simd::f32x4_abs as *const ()) };         // F32x4Abs = 176
    table[177] = unsafe { core::mem::transmute(simd::f32x4_sqrt as *const ()) };        // F32x4Sqrt = 177
    table[178] = unsafe { core::mem::transmute(simd::f32x4_ceil as *const ()) };        // F32x4Ceil = 178
    table[179] = unsafe { core::mem::transmute(simd::f32x4_floor as *const ()) };       // F32x4Floor = 179
    table[180] = unsafe { core::mem::transmute(simd::f32x4_trunc as *const ()) };       // F32x4Trunc = 180
    table[181] = unsafe { core::mem::transmute(simd::f32x4_nearest as *const ()) };     // F32x4Nearest = 181
    table[182] = unsafe { core::mem::transmute(simd::f64x2_neg as *const ()) };         // F64x2Neg = 182
    table[183] = unsafe { core::mem::transmute(simd::f64x2_abs as *const ()) };         // F64x2Abs = 183
    table[184] = unsafe { core::mem::transmute(simd::f64x2_sqrt as *const ()) };        // F64x2Sqrt = 184
    table[185] = unsafe { core::mem::transmute(simd::f64x2_ceil as *const ()) };        // F64x2Ceil = 185
    table[186] = unsafe { core::mem::transmute(simd::f64x2_floor as *const ()) };       // F64x2Floor = 186
    table[187] = unsafe { core::mem::transmute(simd::f64x2_trunc as *const ()) };       // F64x2Trunc = 187
    table[188] = unsafe { core::mem::transmute(simd::f64x2_nearest as *const ()) };     // F64x2Nearest
    // V128Andnot
    table[189] = unsafe { core::mem::transmute(simd::v128_andnot as *const ()) };       // V128Andnot = 189
    // Min/Max
    table[190] = unsafe { core::mem::transmute(simd::i8x16_min_s as *const ()) };      // I8x16MinS = 190
    table[191] = unsafe { core::mem::transmute(simd::i8x16_min_u as *const ()) };      // I8x16MinU = 191
    table[192] = unsafe { core::mem::transmute(simd::i8x16_max_s as *const ()) };      // I8x16MaxS = 192
    table[193] = unsafe { core::mem::transmute(simd::i8x16_max_u as *const ()) };      // I8x16MaxU = 193
    table[194] = unsafe { core::mem::transmute(simd::i16x8_min_s as *const ()) };      // I16x8MinS = 194
    table[195] = unsafe { core::mem::transmute(simd::i16x8_min_u as *const ()) };      // I16x8MinU = 195
    table[196] = unsafe { core::mem::transmute(simd::i16x8_max_s as *const ()) };      // I16x8MaxS = 196
    table[197] = unsafe { core::mem::transmute(simd::i16x8_max_u as *const ()) };      // I16x8MaxU = 197
    table[198] = unsafe { core::mem::transmute(simd::i32x4_min_s as *const ()) };      // I32x4MinS = 198
    table[199] = unsafe { core::mem::transmute(simd::i32x4_min_u as *const ()) };      // I32x4MinU = 199
    table[200] = unsafe { core::mem::transmute(simd::i32x4_max_s as *const ()) };      // I32x4MaxS = 200
    table[201] = unsafe { core::mem::transmute(simd::i32x4_max_u as *const ()) };      // I32x4MaxU = 201
    // Average
    table[202] = unsafe { core::mem::transmute(simd::i8x16_avgr_u as *const ()) };     // I8x16AvgrU = 202
    table[203] = unsafe { core::mem::transmute(simd::i16x8_avgr_u as *const ()) };     // I16x8AvgrU = 203
    // Saturating arithmetic
    table[204] = unsafe { core::mem::transmute(simd::i8x16_add_sat_s as *const ()) };  // I8x16AddSatS = 204
    table[205] = unsafe { core::mem::transmute(simd::i8x16_add_sat_u as *const ()) };  // I8x16AddSatU = 205
    table[206] = unsafe { core::mem::transmute(simd::i8x16_sub_sat_s as *const ()) };  // I8x16SubSatS = 206
    table[207] = unsafe { core::mem::transmute(simd::i8x16_sub_sat_u as *const ()) };  // I8x16SubSatU = 207
    table[208] = unsafe { core::mem::transmute(simd::i16x8_add_sat_s as *const ()) };  // I16x8AddSatS = 208
    table[209] = unsafe { core::mem::transmute(simd::i16x8_add_sat_u as *const ()) };  // I16x8AddSatU = 209
    table[210] = unsafe { core::mem::transmute(simd::i16x8_sub_sat_s as *const ()) };  // I16x8SubSatS = 210
    table[211] = unsafe { core::mem::transmute(simd::i16x8_sub_sat_u as *const ()) };  // I16x8SubSatU = 211
    // Popcnt
    table[212] = unsafe { core::mem::transmute(simd::i8x16_popcnt as *const ()) };     // I8x16Popcnt = 212
    // More reductions
    table[213] = unsafe { core::mem::transmute(simd::v128_bitmask_i16x8 as *const ()) }; // V128BitmaskI16x8 = 213
    table[214] = unsafe { core::mem::transmute(simd::v128_bitmask_i32x4 as *const ()) }; // V128BitmaskI32x4 = 214
    table[215] = unsafe { core::mem::transmute(simd::v128_bitmask_i64x2 as *const ()) }; // V128BitmaskI64x2 = 215
    table[216] = unsafe { core::mem::transmute(simd::v128_all_true_i8x16 as *const ()) }; // V128AllTrueI8x16 = 216
    table[217] = unsafe { core::mem::transmute(simd::v128_all_true_i16x8 as *const ()) }; // V128AllTrueI16x8 = 217
    table[218] = unsafe { core::mem::transmute(simd::v128_all_true_i32x4 as *const ()) }; // V128AllTrueI32x4 = 218
    table[219] = unsafe { core::mem::transmute(simd::v128_all_true_i64x2 as *const ()) }; // V128AllTrueI64x2 = 219
    // Narrowing
    table[220] = unsafe { core::mem::transmute(simd::i8x16_narrow_i16x8_s as *const ()) }; // I8x16NarrowI16x8S = 220
    table[221] = unsafe { core::mem::transmute(simd::i8x16_narrow_i16x8_u as *const ()) }; // I8x16NarrowI16x8U = 221
    table[222] = unsafe { core::mem::transmute(simd::i16x8_narrow_i32x4_s as *const ()) }; // I16x8NarrowI32x4S = 222
    table[223] = unsafe { core::mem::transmute(simd::i16x8_narrow_i32x4_u as *const ()) }; // I16x8NarrowI32x4U = 223
    // Extend
    table[224] = unsafe { core::mem::transmute(simd::i16x8_extend_low_i8x16_s as *const ()) };  // I16x8ExtendLowI8x16S = 224
    table[225] = unsafe { core::mem::transmute(simd::i16x8_extend_high_i8x16_s as *const ()) }; // I16x8ExtendHighI8x16S = 225
    table[226] = unsafe { core::mem::transmute(simd::i16x8_extend_low_i8x16_u as *const ()) };  // I16x8ExtendLowI8x16U = 226
    table[227] = unsafe { core::mem::transmute(simd::i16x8_extend_high_i8x16_u as *const ()) }; // I16x8ExtendHighI8x16U = 227
    table[228] = unsafe { core::mem::transmute(simd::i32x4_extend_low_i16x8_s as *const ()) };  // I32x4ExtendLowI16x8S = 228
    table[229] = unsafe { core::mem::transmute(simd::i32x4_extend_high_i16x8_s as *const ()) }; // I32x4ExtendHighI16x8S = 229
    table[230] = unsafe { core::mem::transmute(simd::i32x4_extend_low_i16x8_u as *const ()) };  // I32x4ExtendLowI16x8U = 230
    table[231] = unsafe { core::mem::transmute(simd::i32x4_extend_high_i16x8_u as *const ()) }; // I32x4ExtendHighI16x8U = 231
    table[232] = unsafe { core::mem::transmute(simd::i64x2_extend_low_i32x4_s as *const ()) };  // I64x2ExtendLowI32x4S = 232
    table[233] = unsafe { core::mem::transmute(simd::i64x2_extend_high_i32x4_s as *const ()) }; // I64x2ExtendHighI32x4S = 233
    table[234] = unsafe { core::mem::transmute(simd::i64x2_extend_low_i32x4_u as *const ()) };  // I64x2ExtendLowI32x4U = 234
    table[235] = unsafe { core::mem::transmute(simd::i64x2_extend_high_i32x4_u as *const ()) }; // I64x2ExtendHighI32x4U = 235
    // Extmul
    table[236] = unsafe { core::mem::transmute(simd::i16x8_extmul_low_i8x16_s as *const ()) };  // I16x8ExtmulLowI8x16S = 236
    table[237] = unsafe { core::mem::transmute(simd::i16x8_extmul_high_i8x16_s as *const ()) }; // I16x8ExtmulHighI8x16S = 237
    table[238] = unsafe { core::mem::transmute(simd::i16x8_extmul_low_i8x16_u as *const ()) };  // I16x8ExtmulLowI8x16U = 238
    table[239] = unsafe { core::mem::transmute(simd::i16x8_extmul_high_i8x16_u as *const ()) }; // I16x8ExtmulHighI8x16U = 239
    table[240] = unsafe { core::mem::transmute(simd::i32x4_extmul_low_i16x8_s as *const ()) };  // I32x4ExtmulLowI16x8S = 240
    table[241] = unsafe { core::mem::transmute(simd::i32x4_extmul_high_i16x8_s as *const ()) }; // I32x4ExtendHighI16x8S = 241
    table[242] = unsafe { core::mem::transmute(simd::i32x4_extmul_low_i16x8_u as *const ()) };  // I32x4ExtmulLowI16x8U = 242
    table[243] = unsafe { core::mem::transmute(simd::i32x4_extmul_high_i16x8_u as *const ()) }; // I32x4ExtmulHighI16x8U = 243
    table[244] = unsafe { core::mem::transmute(simd::i64x2_extmul_low_i32x4_s as *const ()) };  // I64x2ExtmulLowI32x4S = 244
    table[245] = unsafe { core::mem::transmute(simd::i64x2_extmul_high_i32x4_s as *const ()) }; // I64x2ExtmulHighI32x4S = 245
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

    // Special/Internal (460+)
    table[460] = krakeos::krakeos_noop; // __wasi_init_tp, __wasm_call_dtors
    table[463] = krakeos::krakeos_terminal_set_window_size;
    table[464] = krakeos::krakeos_terminal_get_window_size;

    // KrakeOS Container (470+)
    table[470] = krakeos::krakeos_container_plant;
    table[471] = krakeos::krakeos_container_plant_from_path;
    table[472] = krakeos::krakeos_container_harvest;
    table[473] = krakeos::krakeos_container_list_children;
    table[474] = krakeos::krakeos_container_kill_child;

    // KrakeOS Debug (480+)
    table[480] = krakeos::krakeos_debug_get_process_list;
    table[481] = krakeos::krakeos_debug_kill;
    table[482] = krakeos::krakeos_debug_dump_vma;
    table[483] = krakeos::krakeos_debug_get_memory_usage;

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
    table[527] = wasi_p2::wasi_p2_dir_stream_read_directory_entry;

    table[999] = wasi_fs::wasi_serial_print;
    table[1023] = unsafe { core::mem::transmute(traps::process_exit as *const ()) };

    table
};

#[no_mangle]
pub extern "C" fn trampoline_ref_func(ctx: &Ring3Context, func_idx: u32) -> usize {
    unsafe {
        if func_idx >= ctx.func_count { return 0; }
        *ctx.func_table_ptr.add(func_idx as usize) as usize
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { syscall::syscall1(60, 99); } // SYS_EXIT(99)
    loop {}
}

#[no_mangle]
pub extern "C" fn _blob_start() {
    // Placeholder entry point
}
