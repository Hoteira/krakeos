use super::{LOCAL_IP, icmp, udp};

pub fn handle_ipv4(packet: &[u8], eth_src_mac: [u8; 6]) {
    // packet is the Ethernet Payload (IP Header + Data)
    if packet.len() < 20 { return; }

    let ver_ihl = packet[0];
    let version = ver_ihl >> 4;
    let ihl = ver_ihl & 0x0F;
    
    if version != 4 { return; }
    
    let header_len = (ihl * 4) as usize;
    if packet.len() < header_len { return; }

    let protocol = packet[9];
    let src_ip = [packet[12], packet[13], packet[14], packet[15]];
    let dst_ip = [packet[16], packet[17], packet[18], packet[19]];

    // Check if packet is for us or broadcast
    let is_for_us = unsafe { dst_ip == LOCAL_IP };
    let is_broadcast = dst_ip == [255, 255, 255, 255];

    if !is_for_us && !is_broadcast { return; }

    let payload = &packet[header_len..];

    match protocol {
        1 => { // ICMP
            icmp::handle_icmp_with_mac(payload, src_ip, dst_ip, eth_src_mac);
        },
        17 => { // UDP
            udp::handle_udp(payload, src_ip, dst_ip);
        },
        6 => { // TCP
            // TODO: TCP
        },
        _ => {}
    }
}
