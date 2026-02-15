use rust_alloc::vec::Vec;
use super::packet::Packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtherType {
    IPv4,
    ARP,
    IPv6,
    Unknown(u16),
}

impl From<u16> for EtherType {
    fn from(val: u16) -> Self {
        match val {
            0x0800 => EtherType::IPv4,
            0x0806 => EtherType::ARP,
            0x86DD => EtherType::IPv6,
            _ => EtherType::Unknown(val),
        }
    }
}

pub struct EthernetFrame {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ethertype: EtherType,
    pub payload: Vec<u8>,
}

impl EthernetFrame {
    pub fn new(dst: [u8; 6], src: [u8; 6], ethertype: EtherType, payload: Vec<u8>) -> Self {
        Self { dst, src, ethertype, payload }
    }
}

impl Packet for EthernetFrame {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(14 + self.payload.len());
        bytes.extend_from_slice(&self.dst);
        bytes.extend_from_slice(&self.src);
        
        let type_val: u16 = match self.ethertype {
            EtherType::IPv4 => 0x0800,
            EtherType::ARP => 0x0806,
            EtherType::IPv6 => 0x86DD,
            EtherType::Unknown(v) => v,
        };
        
        bytes.push((type_val >> 8) as u8);
        bytes.push((type_val & 0xFF) as u8);
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    fn len(&self) -> usize {
        14 + self.payload.len()
    }
}
