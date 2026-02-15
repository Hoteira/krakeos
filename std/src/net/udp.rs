use rust_alloc::vec::Vec;
use super::packet::Packet;

pub struct UdpPacket {
    pub src_port: u16,
    pub dst_port: u16,
    pub payload: Vec<u8>,
}

impl UdpPacket {
    pub fn new(src_port: u16, dst_port: u16, payload: Vec<u8>) -> Self {
        Self { src_port, dst_port, payload }
    }
}

impl Packet for UdpPacket {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.payload.len());
        
        bytes.push((self.src_port >> 8) as u8);
        bytes.push((self.src_port & 0xFF) as u8);
        
        bytes.push((self.dst_port >> 8) as u8);
        bytes.push((self.dst_port & 0xFF) as u8);
        
        let len = 8 + self.payload.len();
        bytes.push((len >> 8) as u8);
        bytes.push((len & 0xFF) as u8);
        
        // Checksum (0 = Optional for UDP/IPv4)
        bytes.push(0x00); bytes.push(0x00);
        
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    fn len(&self) -> usize {
        8 + self.payload.len()
    }
}
