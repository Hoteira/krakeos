use rust_alloc::vec::Vec;
use super::packet::{Packet, calculate_checksum};

pub struct IcmpPacket {
    pub type_: u8,
    pub code: u8,
    pub id: u16,
    pub seq: u16,
    pub payload: Vec<u8>,
}

impl IcmpPacket {
    pub fn new_echo_request(id: u16, seq: u16, payload: Vec<u8>) -> Self {
        Self {
            type_: 8,
            code: 0,
            id,
            seq,
            payload,
        }
    }
    
    pub fn new_echo_reply(id: u16, seq: u16, payload: Vec<u8>) -> Self {
        Self {
            type_: 0,
            code: 0,
            id,
            seq,
            payload,
        }
    }
}

impl Packet for IcmpPacket {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.payload.len());
        
        bytes.push(self.type_);
        bytes.push(self.code);
        // Checksum (placeholder)
        bytes.push(0x00); bytes.push(0x00);
        
        bytes.push((self.id >> 8) as u8);
        bytes.push((self.id & 0xFF) as u8);
        
        bytes.push((self.seq >> 8) as u8);
        bytes.push((self.seq & 0xFF) as u8);
        
        bytes.extend_from_slice(&self.payload);
        
        let checksum = calculate_checksum(&bytes);
        bytes[2] = (checksum >> 8) as u8;
        bytes[3] = (checksum & 0xFF) as u8;
        
        bytes
    }

    fn len(&self) -> usize {
        8 + self.payload.len()
    }
}
