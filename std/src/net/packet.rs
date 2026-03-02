use alloc::vec::Vec;

pub trait Packet {
    fn to_bytes(&self) -> Vec<u8>;
    fn len(&self) -> usize;
}

pub fn calculate_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for chunk in data.chunks(2) {
        let word = if chunk.len() == 2 {
            ((chunk[0] as u32) << 8) | (chunk[1] as u32)
        } else {
            (chunk[0] as u32) << 8
        };
        sum = sum.wrapping_add(word);
    }
    
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    
    !sum as u16
}
