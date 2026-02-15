use alloc::vec::Vec;
use crate::drivers::network::virtio::send_packet;
use std::net::packet::Packet;
use std::net::ethernet::{EthernetFrame, EtherType};
use std::net::arp::ArpPacket;
use super::{LOCAL_IP, LOCAL_MAC};

pub fn handle_arp(packet: &[u8]) {
    // packet is full Ethernet frame
    if packet.len() < 42 { return; }
    
    // Eth Header is 0..14
    // ARP starts at 14
    let arp_base = 14;
    
    let oper = ((packet[arp_base + 6] as u16) << 8) | packet[arp_base + 7] as u16;
    let target_ip = &packet[arp_base + 24 .. arp_base + 28];
    
    unsafe {
        let local_ip = *(&raw const LOCAL_IP);
        let local_mac = *(&raw const LOCAL_MAC);
        if oper == 1 && target_ip == local_ip {
            crate::debugln!("[Kernel] ARP Request for {:?} from {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}", 
                local_ip, packet[6], packet[7], packet[8], packet[9], packet[10], packet[11]);

            // Construct Reply
            let sender_ha = &packet[arp_base + 8 .. arp_base + 14]; // Sender MAC
            let sender_ip = &packet[arp_base + 14 .. arp_base + 18]; // Sender IP
            
            let arp_reply = ArpPacket::new_reply(&local_mac, &local_ip, sender_ha, sender_ip);
            
            // Wrap in Ethernet
            let mut dst_mac = [0u8; 6];
            dst_mac.copy_from_slice(sender_ha);
            
            let eth = EthernetFrame::new(dst_mac, local_mac, EtherType::ARP, arp_reply.to_bytes());
            
            send_packet(&eth.to_bytes());
        }
    }
}