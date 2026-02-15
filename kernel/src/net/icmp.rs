use alloc::vec::Vec;
use crate::drivers::network::virtio::send_packet;
use std::net::packet::Packet;
use std::net::ethernet::{EthernetFrame, EtherType};
use std::net::ipv4::{Ipv4Packet, IpProto};
use std::net::icmp::IcmpPacket;
use super::{LOCAL_IP, LOCAL_MAC};

pub fn handle_icmp(packet: &[u8], src_ip: [u8; 4], dst_ip: [u8; 4]) {
    // packet is the IP Payload (ICMP header + data)
    if packet.len() < 8 { return; }

    let type_ = packet[0];
    let code = packet[1];
    let _checksum = ((packet[2] as u16) << 8) | packet[3] as u16;
    let id = ((packet[4] as u16) << 8) | packet[5] as u16;
    let seq = ((packet[6] as u16) << 8) | packet[7] as u16;

    // Echo Request
    if type_ == 8 && code == 0 {
        crate::debugln!("[Kernel] ICMP Echo Request from {:?} id={} seq={}", src_ip, id, seq);

        let payload = &packet[8..];
        
        // Construct Echo Reply
        let reply = IcmpPacket::new_echo_reply(id, seq, payload.to_vec());
        
        // Wrap in IPv4 (Swap src/dst)
        // Note: In a real stack we'd check routing table, here we just reply to sender
        let ip = Ipv4Packet::new(unsafe { LOCAL_IP }, src_ip, IpProto::ICMP, reply.to_bytes());
        
        // We need the destination MAC. 
        // In a real stack, we'd check the ARP Cache.
        // For now, we don't have the MAC passed down easily from the Eth frame without breaking layers or lookup.
        // HACK: We will Broadcast it or need to look it up.
        // Better HACK: For the "Reply" flow, we usually assume the Eth layer handled it or we have a cache.
        // Since we are stateless, let's cheat and assume we can reply to the MAC that sent it?
        // But `handle_icmp` doesn't have the Eth header.
        
        // SOLUTION: Let's assume for this specific test environment (QEMU user net), 
        // the gateway 10.0.2.2 is at 52:55:0a:00:02:02 usually.
        // BUT! `handle_ipv4` calls us. `handle_ipv4` is called by `on_receive` which has the Eth packet.
        // We should probably pass the source MAC down or implement an ARP cache.
        
        // Let's implement a tiny static ARP Cache or pass the MAC through.
        // Passing MAC through is cleaner for this stage.
    }
}

pub fn handle_icmp_with_mac(packet: &[u8], src_ip: [u8; 4], dst_ip: [u8; 4], src_mac: [u8; 6]) {
    if packet.len() < 8 { return; }

    let type_ = packet[0];
    let code = packet[1];
    let id = ((packet[4] as u16) << 8) | packet[5] as u16;
    let seq = ((packet[6] as u16) << 8) | packet[7] as u16;

    if type_ == 8 && code == 0 {
        // crate::debugln!("[Kernel] ICMP Ping from {:?} id={} seq={}", src_ip, id, seq);

        let payload = &packet[8..];
        let reply = IcmpPacket::new_echo_reply(id, seq, payload.to_vec());
        let ip = Ipv4Packet::new(unsafe { LOCAL_IP }, src_ip, IpProto::ICMP, reply.to_bytes());
        let eth = EthernetFrame::new(src_mac, unsafe { LOCAL_MAC }, EtherType::IPv4, ip.to_bytes());
        
        send_packet(&eth.to_bytes());
    }
}
