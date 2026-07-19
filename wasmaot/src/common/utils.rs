//! Debug helpers (stubbed in the KrakeOS port).

#[cfg(debug_assertions)]
pub fn print_beautiful_instruction_name_1_byte(_first_byte: u8, _pc: usize) {}

#[cfg(debug_assertions)]
pub fn print_beautiful_fc_extension(_second_byte: u32, _pc: usize) {}

#[cfg(debug_assertions)]
pub fn print_beautiful_fd_extension(_second_byte: u32, _pc: usize) {}
