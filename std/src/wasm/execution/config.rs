pub trait Config {
    const MAX_VALUE_STACK_SIZE: usize = 0xf0000;
    const MAX_CALL_STACK_SIZE: usize = 0x1000;
    #[allow(unused_variables)]
    #[inline(always)]
    fn instruction_hook(&mut self, bytecode: &[u8], pc: usize) {}
    #[inline(always)]
    fn get_flat_cost(_instr: u8) -> u32 {
        1
    }
    #[inline(always)]
    fn get_fc_extension_flat_cost(_instr: u32) -> u32 {
        1
    }
    #[inline(always)]
    fn get_fd_extension_flat_cost(_instr: u32) -> u32 {
        1
    }
    #[inline(always)]
    fn get_cost_per_element(_instr: u8) -> u32 {
        0
    }
    #[inline(always)]
    fn get_fc_extension_cost_per_element(_instr: u32) -> u32 {
        0
    }
}
impl Config for () {}
