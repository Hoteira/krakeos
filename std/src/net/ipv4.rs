use rust_alloc::vec::Vec;
use super::packet::{Packet, calculate_checksum};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpProto {
    ICMP,
    TCP,
    UDP,
    Unknown(u8),
}

impl From<u8> for IpProto {
    fn from(val: u8) -> Self {
        match val {
            1 => IpProto::ICMP,
            6 => IpProto::TCP,
            17 => IpProto::UDP,
            _ => IpProto::Unknown(val),
        }
    }
}

pub struct Ipv4Packet {
    pub src: [u8; 4],
    pub dst: [u8; 4],
    pub proto: IpProto,
    pub payload: Vec<u8>,
}

impl Ipv4Packet {
    pub fn new(src: [u8; 4], dst: [u8; 4], proto: IpProto, payload: Vec<u8>) -> Self {
        Self { src, dst, proto, payload }
    }
}

impl Packet for Ipv4Packet {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(20 + self.payload.len());
        
        // Version (4) + IHL (5)
        bytes.push(0x45);
        // TOS
        bytes.push(0x00);
        // Total Length
        let total_len = 20 + self.payload.len();
        bytes.push((total_len >> 8) as u8);
        bytes.push((total_len & 0xFF) as u8);
        // ID
        bytes.push(0x00); bytes.push(0x00);
        // Flags + Fragment Offset
        bytes.push(0x00); bytes.push(0x00);
        // TTL
        bytes.push(64);
        // Protocol
        let proto_val: u8 = match self.proto {
            IpProto::ICMP => 1,
            IpProto::TCP => 6,
            IpProto::UDP => 17,
            IpProto::Unknown(v) => v,
        };
        bytes.push(proto_val);
        // Checksum (placeholder)
        bytes.push(0x00); bytes.push(0x00);
        // Src IP
        bytes.extend_from_slice(&self.src);
        // Dst IP
        bytes.extend_from_slice(&self.dst);
        
        // Calculate Checksum
        let checksum = calculate_checksum(&bytes[0..20]);
        bytes[10] = (checksum >> 8) as u8;
        bytes[11] = (checksum & 0xFF) as u8;
        
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    fn len(&self) -> usize {
        20 + self.payload.len()
    }
}
