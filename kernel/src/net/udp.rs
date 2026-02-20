use alloc::vec::Vec;
use crate::drivers::network::virtio::send_packet;
use std::net::packet::Packet;
use std::net::ethernet::{EthernetFrame, EtherType};
use std::net::ipv4::{Ipv4Packet, IpProto};
use std::net::udp::UdpPacket;
use super::{LOCAL_IP, LOCAL_MAC};
use super::socket::SOCKET_MANAGER;

pub fn handle_udp(packet: &[u8], src_ip: [u8; 4], dst_ip: [u8; 4]) {
    // packet is UDP Header + Data
    if packet.len() < 8 { return; }

    let src_port = ((packet[0] as u16) << 8) | packet[1] as u16;
    let dst_port = ((packet[2] as u16) << 8) | packet[3] as u16;
    let length = ((packet[4] as u16) << 8) | packet[5] as u16;
    
    if packet.len() < length as usize { return; }

    let payload = &packet[8..length as usize];
    
    // crate::debugln!("[Kernel] UDP Recv: {} -> {} ({} bytes)", src_port, dst_port, payload.len());

    // Dispatch to Socket
    // We need to preserve sender info (src_ip, src_port) for recvfrom to be useful.
    // For now, let's just push the payload.
    // Real implementation: push (src_ip, src_port, payload) struct.
    
    // Simplification: Prepend 6 bytes (IP:4 + Port:2) to payload in queue
    let mut internal_packet = Vec::with_capacity(6 + payload.len());
    internal_packet.extend_from_slice(&src_ip);
    internal_packet.push((src_port >> 8) as u8);
    internal_packet.push((src_port & 0xFF) as u8);
    internal_packet.extend_from_slice(payload);

    SOCKET_MANAGER.lock().push_packet(dst_port, internal_packet);
}

pub fn send_udp(src_port: u16, dst_ip: [u8; 4], dst_port: u16, payload: &[u8]) {
    // 1. Build UDP
    let udp = UdpPacket::new(src_port, dst_port, payload.to_vec());
    let udp_bytes = udp.to_bytes();

    // Loopback check
    let is_loopback = dst_ip == [127, 0, 0, 1] || dst_ip == unsafe { LOCAL_IP };
    if is_loopback {
        // Direct dispatch to local socket
        // src_ip should be LOCAL_IP or 127.0.0.1
        let src_ip = if dst_ip == [127, 0, 0, 1] { [127, 0, 0, 1] } else { unsafe { LOCAL_IP } };
        handle_udp(&udp_bytes, src_ip, dst_ip);
        return;
    }
    
    // 2. Build IPv4
    let ip = Ipv4Packet::new(unsafe { LOCAL_IP }, dst_ip, IpProto::UDP, udp_bytes);
    
    // 3. Resolve MAC (ARP)
    // HACK: For now, if broadcast IP, use broadcast MAC. 
    // If unicast, assume Gateway/Host MAC (52:55:0a:00:02:02) or Broadcast if unknown.
    // Real stack needs ARP Cache lookup.
    
    let dst_mac = if dst_ip == [255, 255, 255, 255] {
        [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
    } else {
        // Assume QEMU Gateway MAC for 10.0.2.2, or Broadcast fallback
        // Let's just broadcast unicast packets for now to ensure delivery in test env
        // or hardcode gateway MAC.
        // Gateway 10.0.2.2 usually is 52:55:0a:00:02:02
        [0x52, 0x55, 0x0a, 0x00, 0x02, 0x02]
    };
    
    // 4. Build Ethernet
    let eth = EthernetFrame::new(dst_mac, unsafe { LOCAL_MAC }, EtherType::IPv4, ip.to_bytes());
    
    send_packet(&eth.to_bytes());
}
