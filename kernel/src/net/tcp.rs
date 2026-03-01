#![allow(static_mut_refs)]
use super::{LOCAL_IP, LOCAL_MAC};
use crate::sync::Mutex;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnKey {
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Listen, SynSent, SynReceived, Established, FinWait1, FinWait2, CloseWait, LastAck, Closed,
}

pub struct TcpConnection {
    pub state: TcpState,
    pub key: ConnKey,
    pub snd_nxt: u32,
    pub rcv_nxt: u32,
    pub rx_buf: VecDeque<u8>,
    pub socket_id: usize,
}

pub struct TcpManager {
    pub connections: BTreeMap<ConnKey, TcpConnection>,
    pub listeners: BTreeMap<u16, usize>,
    pub accept_queue: BTreeMap<usize, VecDeque<ConnKey>>,
}

pub static TCP_MANAGER: Mutex<TcpManager> = Mutex::new(TcpManager {
    connections: BTreeMap::new(),
    listeners: BTreeMap::new(),
    accept_queue: BTreeMap::new(),
});

pub fn send_tcp_segment(src_port: u16, dst_ip: [u8; 4], dst_port: u16, seq: u32, ack: u32, flags: u8, payload: &[u8]) {
    let mut tcp_hdr = [0u8; 20];
    tcp_hdr[0..2].copy_from_slice(&src_port.to_be_bytes());
    tcp_hdr[2..4].copy_from_slice(&dst_port.to_be_bytes());
    tcp_hdr[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp_hdr[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp_hdr[12] = (20u8 / 4) << 4;
    tcp_hdr[13] = flags;
    tcp_hdr[14..16].copy_from_slice(&65535u16.to_be_bytes());

    let src_ip = if dst_ip == [127, 0, 0, 1] { [127, 0, 0, 1] } else { unsafe { LOCAL_IP } };
    let chk = tcp_checksum(src_ip, dst_ip, &tcp_hdr, payload);
    tcp_hdr[16..18].copy_from_slice(&chk.to_be_bytes());

    let mut tcp_seg = alloc::vec![0u8; 20 + payload.len()];
    tcp_seg[..20].copy_from_slice(&tcp_hdr);
    tcp_seg[20..].copy_from_slice(payload);

    send_ip_packet(dst_ip, 6, &tcp_seg);
}

fn send_ip_packet(dst_ip: [u8; 4], _proto: u8, payload: &[u8]) {
    use std::net::ethernet::{EtherType, EthernetFrame};
    use std::net::ipv4::{IpProto, Ipv4Packet};
    use std::net::packet::Packet;
    
    let src_ip = if dst_ip == [127, 0, 0, 1] { [127, 0, 0, 1] } else { unsafe { LOCAL_IP } };
    let ip = Ipv4Packet::new(src_ip, dst_ip, IpProto::TCP, payload.to_vec());
    let ip_bytes = ip.to_bytes();

    if dst_ip == [127, 0, 0, 1] || dst_ip == src_ip {
        let local_mac = unsafe { LOCAL_MAC };
        let mut eth = alloc::vec![0u8; 14 + ip_bytes.len()];
        eth[0..6].copy_from_slice(&local_mac); eth[6..12].copy_from_slice(&local_mac);
        eth[12] = 0x08; eth[13] = 0x00; eth[14..].copy_from_slice(&ip_bytes);
        crate::net::push_loopback_packet(eth);
        return;
    }

    let eth = EthernetFrame::new([0x52, 0x55, 0x0a, 0x00, 0x02, 0x02], unsafe { LOCAL_MAC }, EtherType::IPv4, ip_bytes);
    crate::drivers::network::virtio::send_packet(&eth.to_bytes());
}

fn tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], hdr: &[u8; 20], data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for i in 0..2 { sum += u16::from_be_bytes([src_ip[i * 2], src_ip[i * 2 + 1]]) as u32; }
    for i in 0..2 { sum += u16::from_be_bytes([dst_ip[i * 2], dst_ip[i * 2 + 1]]) as u32; }
    sum += 6 + (20 + data.len()) as u32;
    for chunk in hdr.chunks(2).chain(data.chunks(2)) {
        sum += if chunk.len() == 2 { u16::from_be_bytes([chunk[0], chunk[1]]) as u32 } else { (chunk[0] as u32) << 8 };
    }
    while sum >> 16 != 0 { sum = (sum & 0xFFFF) + (sum >> 16); }
    !(sum as u16)
}

pub fn handle_tcp(packet: &[u8], src_ip: [u8; 4]) {
    if packet.len() < 20 { return; }
    let src_port = u16::from_be_bytes([packet[0], packet[1]]);
    let dst_port = u16::from_be_bytes([packet[2], packet[3]]);
    let seq = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
    let ack_num = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let flags = packet[13];
    let data = &packet[((packet[12] >> 4) * 4) as usize..];
    let key = ConnKey { local_port: dst_port, remote_ip: src_ip, remote_port: src_port };

    let mut mgr = TCP_MANAGER.int_lock();
    if flags & TCP_FLAG_SYN != 0 && flags & TCP_FLAG_ACK == 0 {
        if let Some(&socket_id) = mgr.listeners.get(&dst_port) {
            let isn = pseudo_random_isn(src_ip, src_port, dst_port);
            mgr.connections.insert(key, TcpConnection { state: TcpState::SynReceived, key, snd_nxt: isn.wrapping_add(1), rcv_nxt: seq.wrapping_add(1), rx_buf: VecDeque::new(), socket_id });
            drop(mgr);
            send_tcp_segment(dst_port, src_ip, src_port, isn, seq.wrapping_add(1), TCP_FLAG_SYN | TCP_FLAG_ACK, &[]);
            return;
        }
        drop(mgr); send_tcp_segment(dst_port, src_ip, src_port, 0, seq.wrapping_add(1), TCP_FLAG_RST | TCP_FLAG_ACK, &[]);
        return;
    }

    let mut action = None;
    if let Some(conn) = mgr.connections.get_mut(&key) {
        match conn.state {
            TcpState::SynReceived => if flags & TCP_FLAG_ACK != 0 {
                conn.state = TcpState::Established;
                action = Some((conn.socket_id, "established"));
            },
            TcpState::SynSent => if flags & (TCP_FLAG_SYN | TCP_FLAG_ACK) == (TCP_FLAG_SYN | TCP_FLAG_ACK) {
                conn.rcv_nxt = seq.wrapping_add(1); conn.state = TcpState::Established;
                let (sn, rn) = (conn.snd_nxt, conn.rcv_nxt);
                drop(mgr);
                send_tcp_segment(dst_port, src_ip, src_port, sn, rn, TCP_FLAG_ACK, &[]);
                return;
            },
            TcpState::Established => {
                if !data.is_empty() {
                    conn.rx_buf.extend(data.iter().copied());
                    let sid = conn.socket_id;
                    conn.rcv_nxt = conn.rcv_nxt.wrapping_add(data.len() as u32);
                    let (sn, rn, lp) = (conn.snd_nxt, conn.rcv_nxt, conn.key.local_port);
                    drop(mgr);
                    if sid != 0 {
                        let mut sm = crate::net::socket::SOCKET_MANAGER.lock();
                        if let Some(s) = sm.sockets.get_mut(&sid) { s.rx_queue.push(data.to_vec()); }
                    }
                    send_tcp_segment(lp, src_ip, src_port, sn, rn, TCP_FLAG_ACK, &[]);
                    return;
                }
                if flags & TCP_FLAG_FIN != 0 {
                    conn.state = TcpState::CloseWait; conn.rcv_nxt = conn.rcv_nxt.wrapping_add(1);
                    let (sn, rn, lp) = (conn.snd_nxt, conn.rcv_nxt, conn.key.local_port);
                    drop(mgr);
                    send_tcp_segment(lp, src_ip, src_port, sn, rn, TCP_FLAG_ACK, &[]);
                    return;
                }
            },
            _ => {}
        }
    }

    if let Some((sid, kind)) = action {
        if kind == "established" {
            mgr.accept_queue.entry(sid).or_default().push_back(key);
        }
    }
}

pub fn tcp_connect_start(local_port: u16, dst_ip: [u8; 4], dst_port: u16, socket_id: usize) -> Result<ConnKey, &'static str> {
    let key = ConnKey { local_port, remote_ip: dst_ip, remote_port: dst_port };
    let isn = pseudo_random_isn(dst_ip, local_port, dst_port);
    TCP_MANAGER.int_lock().connections.insert(key, TcpConnection { state: TcpState::SynSent, key, snd_nxt: isn.wrapping_add(1), rcv_nxt: 0, rx_buf: VecDeque::new(), socket_id });
    send_tcp_segment(local_port, dst_ip, dst_port, isn, 0, TCP_FLAG_SYN, &[]);
    Ok(key)
}

pub fn tcp_connect_check(key: ConnKey) -> bool {
    let mgr = TCP_MANAGER.int_lock();
    mgr.connections.get(&key).map(|c| c.state == TcpState::Established).unwrap_or(false)
}

pub fn tcp_connect(local_port: u16, dst_ip: [u8; 4], dst_port: u16, socket_id: usize) -> Result<(), &'static str> {
    let key = tcp_connect_start(local_port, dst_ip, dst_port, socket_id)?;
    for _ in 0..50000 {
        crate::drivers::network::virtio::poll_rx();
        if tcp_connect_check(key) { return Ok(()); }
        unsafe { core::arch::asm!("int 0x81"); }
    }
    Err("connect timeout")
}

pub fn tcp_listen(local_port: u16, socket_id: usize) {
    let mut mgr = TCP_MANAGER.int_lock();
    mgr.listeners.insert(local_port, socket_id);
    mgr.accept_queue.entry(socket_id).or_default();
}

pub fn tcp_accept(socket_id: usize) -> Option<ConnKey> {
    TCP_MANAGER.int_lock().accept_queue.get_mut(&socket_id)?.pop_front()
}

pub fn tcp_send(key: ConnKey, data: &[u8]) -> Result<usize, &'static str> {
    let (sn, rn) = {
        let mut mgr = TCP_MANAGER.int_lock();
        let conn = mgr.connections.get_mut(&key).ok_or("no connection")?;
        if conn.state != TcpState::Established { return Err("not established"); }
        let (sn, rn) = (conn.snd_nxt, conn.rcv_nxt);
        conn.snd_nxt = conn.snd_nxt.wrapping_add(data.len() as u32);
        (sn, rn)
    };
    send_tcp_segment(key.local_port, key.remote_ip, key.remote_port, sn, rn, TCP_FLAG_ACK | TCP_FLAG_PSH, data);
    Ok(data.len())
}

pub fn tcp_recv(key: ConnKey, buf: &mut [u8]) -> usize {
    crate::drivers::network::virtio::poll_rx();
    let mut mgr = TCP_MANAGER.int_lock();
    if let Some(conn) = mgr.connections.get_mut(&key) {
        let n = core::cmp::min(buf.len(), conn.rx_buf.len());
        for i in 0..n { buf[i] = conn.rx_buf.pop_front().unwrap(); }
        return n;
    }
    0
}

pub fn tcp_close(key: ConnKey) {
    if let Some((sn, rn)) = {
        let mut mgr = TCP_MANAGER.int_lock();
        mgr.connections.get_mut(&key).map(|c| { c.state = TcpState::FinWait1; (c.snd_nxt, c.rcv_nxt) })
    } {
        send_tcp_segment(key.local_port, key.remote_ip, key.remote_port, sn, rn, TCP_FLAG_FIN | TCP_FLAG_ACK, &[]);
    }
}

static mut ISN_COUNTER: u32 = 0;
fn pseudo_random_isn(ip: [u8; 4], src_port: u16, dst_port: u16) -> u32 {
    let mut v = 0xA5A5A5A5u32; unsafe { ISN_COUNTER = ISN_COUNTER.wrapping_add(1); v ^= ISN_COUNTER; }
    v ^= (ip[0] as u32) << 24 | (ip[1] as u32) << 16 | (ip[2] as u32) << 8 | ip[3] as u32;
    v ^= (src_port as u32) << 16 | dst_port as u32;
    v = v.wrapping_mul(0x9e3779b9); v ^= v >> 16; v
}

pub fn conn_key_pack(key: ConnKey) -> u64 { ((key.local_port as u64) << 48) | ((key.remote_port as u64) << 32) | ((key.remote_ip[0] as u64) << 24) | ((key.remote_ip[1] as u64) << 16) | ((key.remote_ip[2] as u64) << 8) | (key.remote_ip[3] as u64) }
pub fn conn_key_unpack(v: u64) -> ConnKey { ConnKey { local_port: ((v >> 48) & 0xFFFF) as u16, remote_port: ((v >> 32) & 0xFFFF) as u16, remote_ip: [((v >> 24) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, (v & 0xFF) as u8] } }
