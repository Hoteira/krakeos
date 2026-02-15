use rust_alloc::vec::Vec;
use super::packet::Packet;

pub struct ArpPacket {
    pub htype: u16,
    pub ptype: u16,
    pub hlen: u8,
    pub plen: u8,
    pub oper: u16,
    pub sender_ha: Vec<u8>,
    pub sender_ip: Vec<u8>,
    pub target_ha: Vec<u8>,
    pub target_ip: Vec<u8>,
}

impl ArpPacket {
    pub fn new_reply(sender_ha: &[u8], sender_ip: &[u8], target_ha: &[u8], target_ip: &[u8]) -> Self {
        Self {
            htype: 1, // Ethernet
            ptype: 0x0800, // IPv4
            hlen: 6,
            plen: 4,
            oper: 2, // Reply
            sender_ha: sender_ha.to_vec(),
            sender_ip: sender_ip.to_vec(),
            target_ha: target_ha.to_vec(),
            target_ip: target_ip.to_vec(),
        }
    }
}

impl Packet for ArpPacket {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.push((self.htype >> 8) as u8); bytes.push((self.htype & 0xFF) as u8);
        bytes.push((self.ptype >> 8) as u8); bytes.push((self.ptype & 0xFF) as u8);
        bytes.push(self.hlen);
        bytes.push(self.plen);
        bytes.push((self.oper >> 8) as u8); bytes.push((self.oper & 0xFF) as u8);
        bytes.extend_from_slice(&self.sender_ha);
        bytes.extend_from_slice(&self.sender_ip);
        bytes.extend_from_slice(&self.target_ha);
        bytes.extend_from_slice(&self.target_ip);
        bytes
    }

    fn len(&self) -> usize {
        8 + (self.hlen as usize * 2) + (self.plen as usize * 2)
    }
}
