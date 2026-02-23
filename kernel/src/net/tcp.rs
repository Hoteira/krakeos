#![allow(static_mut_refs)]
use super::{LOCAL_IP, LOCAL_MAC};
use crate::sync::Mutex;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

// ────────────────────────────────────────────────────────────
// TCP Header constants
// ────────────────────────────────────────────────────────────
pub const TCP_FLAG_FIN: u8 = 0x01;
pub const TCP_FLAG_SYN: u8 = 0x02;
pub const TCP_FLAG_RST: u8 = 0x04;
pub const TCP_FLAG_PSH: u8 = 0x08;
pub const TCP_FLAG_ACK: u8 = 0x10;

// ────────────────────────────────────────────────────────────
// Connection key: uniquely identifies a TCP flow
// ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnKey {
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
}

/// State of a single TCP connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    LastAck,
    Closed,
}

pub struct TcpConnection {
    pub state: TcpState,
    pub key: ConnKey,
    /// Our sequence number (next byte we'll send)
    pub snd_nxt: u32,
    /// ACK number we've sent (next byte we expect from remote)
    pub rcv_nxt: u32,
    /// Data received from peer, ready to be read
    pub rx_buf: VecDeque<u8>,
    /// Socket id owning this connection (for wakeup, optional)
    pub socket_id: usize,
}

pub struct TcpManager {
    /// Active connections: ConnKey → TcpConnection
    pub connections: BTreeMap<ConnKey, TcpConnection>,
    /// Listening sockets: local_port → socket_id
    pub listeners: BTreeMap<u16, usize>,
    /// Pending accepted connections waiting for accept(): socket_id → queue
    pub accept_queue: BTreeMap<usize, VecDeque<ConnKey>>,
}

pub static TCP_MANAGER: Mutex<TcpManager> = Mutex::new(TcpManager {
    connections: BTreeMap::new(),
    listeners: BTreeMap::new(),
    accept_queue: BTreeMap::new(),
});

// ────────────────────────────────────────────────────────────
// Packet building helpers
// ────────────────────────────────────────────────────────────

/// Build a raw TCP segment (no IP header) and send it via the network stack.
pub fn send_tcp_segment(
    src_port: u16,
    dst_ip: [u8; 4],
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) {
    // TCP header (20 bytes, no options)
    let data_offset = (20u8 / 4) << 4;
    let mut tcp_hdr = [0u8; 20];
    tcp_hdr[0..2].copy_from_slice(&src_port.to_be_bytes());
    tcp_hdr[2..4].copy_from_slice(&dst_port.to_be_bytes());
    tcp_hdr[4..8].copy_from_slice(&seq.to_be_bytes());
    tcp_hdr[8..12].copy_from_slice(&ack.to_be_bytes());
    tcp_hdr[12] = data_offset;
    tcp_hdr[13] = flags;
    tcp_hdr[14..16].copy_from_slice(&65535u16.to_be_bytes()); // window
    // checksum placeholder (bytes 16-18 stay 0 until filled)

    let tcp_len = 20 + payload.len();

    // Pseudo-header checksum
    let src_ip = if dst_ip == [127, 0, 0, 1] {
        [127, 0, 0, 1]
    } else {
        unsafe { LOCAL_IP }
    };
    let chk = tcp_checksum(src_ip, dst_ip, &tcp_hdr, payload);
    tcp_hdr[16..18].copy_from_slice(&chk.to_be_bytes());

    // Assemble TCP segment
    let mut tcp_seg = alloc::vec![0u8; tcp_len];
    tcp_seg[..20].copy_from_slice(&tcp_hdr);
    tcp_seg[20..].copy_from_slice(payload);

    send_ip_packet(dst_ip, 6, &tcp_seg);
}

fn send_ip_packet(dst_ip: [u8; 4], _proto: u8, payload: &[u8]) {
    use std::net::ethernet::{EtherType, EthernetFrame};
    use std::net::ipv4::{IpProto, Ipv4Packet};
    use std::net::packet::Packet;
    let src_ip = if dst_ip == [127, 0, 0, 1] {
        [127, 0, 0, 1]
    } else {
        unsafe { LOCAL_IP }
    };

    let is_loopback = dst_ip == [127, 0, 0, 1] || dst_ip == src_ip;

    let ip = Ipv4Packet::new(src_ip, dst_ip, IpProto::TCP, payload.to_vec());
    let ip_bytes = ip.to_bytes();

    if is_loopback {
        let local_mac = unsafe { LOCAL_MAC };
        let mut eth_fake = alloc::vec![0u8; 14 + ip_bytes.len()];
        eth_fake[0..6].copy_from_slice(&local_mac);
        eth_fake[6..12].copy_from_slice(&local_mac);
        eth_fake[12] = 0x08;
        eth_fake[13] = 0x00;
        eth_fake[14..].copy_from_slice(&ip_bytes);
        crate::net::push_loopback_packet(eth_fake);
        return;
    }

    let dst_mac = [0x52u8, 0x55, 0x0a, 0x00, 0x02, 0x02];
    let eth = EthernetFrame::new(dst_mac, unsafe { LOCAL_MAC }, EtherType::IPv4, ip_bytes);
    crate::drivers::network::virtio::send_packet(&eth.to_bytes());
}

/// RFC 793 checksum over TCP pseudo-header + segment
fn tcp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], hdr: &[u8; 20], data: &[u8]) -> u16 {
    let tcp_len = (hdr.len() + data.len()) as u16;
    let mut sum: u32 = 0;

    // Pseudo-header
    for i in 0..2 {
        sum += u16::from_be_bytes([src_ip[i * 2], src_ip[i * 2 + 1]]) as u32;
    }
    for i in 0..2 {
        sum += u16::from_be_bytes([dst_ip[i * 2], dst_ip[i * 2 + 1]]) as u32;
    }
    sum += 6u32; // protocol = TCP
    sum += tcp_len as u32;

    // Header + data (treat as big-endian u16 pairs)
    let iter = hdr.chunks(2).chain(data.chunks(2));
    for chunk in iter {
        let word = if chunk.len() == 2 {
            u16::from_be_bytes([chunk[0], chunk[1]]) as u32
        } else {
            (chunk[0] as u32) << 8
        };
        sum += word;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

// ────────────────────────────────────────────────────────────
// Incoming packet handler — called from ipv4::handle_ipv4
// ────────────────────────────────────────────────────────────
pub fn handle_tcp(packet: &[u8], src_ip: [u8; 4]) {
    if packet.len() < 20 {
        return;
    }

    let src_port = u16::from_be_bytes([packet[0], packet[1]]);
    let dst_port = u16::from_be_bytes([packet[2], packet[3]]);
    let seq = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
    let ack_num = u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
    let data_off = ((packet[12] >> 4) * 4) as usize;
    let flags = packet[13];

    if data_off > packet.len() {
        return;
    }
    let data = &packet[data_off..];

    let key = ConnKey {
        local_port: dst_port,
        remote_ip: src_ip,
        remote_port: src_port,
    };

    let mut mgr = TCP_MANAGER.lock();

    // ── SYN on a listening port → SYN-ACK ──────────────────
    if flags & TCP_FLAG_SYN != 0 && flags & TCP_FLAG_ACK == 0 {
        if let Some(&socket_id) = mgr.listeners.get(&dst_port) {
            let isn = pseudo_random_isn(src_ip, src_port, dst_port);
            let conn = TcpConnection {
                state: TcpState::SynReceived,
                key,
                snd_nxt: isn.wrapping_add(1),
                rcv_nxt: seq.wrapping_add(1),
                rx_buf: VecDeque::new(),
                socket_id,
            };
            mgr.connections.insert(key, conn);
            drop(mgr);
            send_tcp_segment(
                dst_port,
                src_ip,
                src_port,
                isn,
                seq.wrapping_add(1),
                TCP_FLAG_SYN | TCP_FLAG_ACK,
                &[],
            );
            return;
        }
        // RST for unknown ports
        drop(mgr);
        send_tcp_segment(
            dst_port,
            src_ip,
            src_port,
            0,
            seq.wrapping_add(1),
            TCP_FLAG_RST | TCP_FLAG_ACK,
            &[],
        );
        return;
    }

    // ── Handle existing connection ──────────────────────────
    if let Some(conn) = mgr.connections.get_mut(&key) {
        match conn.state {
            TcpState::SynReceived => {
                if flags & TCP_FLAG_ACK != 0 {
                    conn.state = TcpState::Established;
                    let socket_id = conn.socket_id;
                    // Push to accept queue
                    mgr.accept_queue
                        .entry(socket_id)
                        .or_insert_with(VecDeque::new)
                        .push_back(key);
                }
            }
            TcpState::SynSent => {
                if flags & (TCP_FLAG_SYN | TCP_FLAG_ACK) == (TCP_FLAG_SYN | TCP_FLAG_ACK) {
                    conn.rcv_nxt = seq.wrapping_add(1);
                    conn.state = TcpState::Established;
                    let snd_nxt = conn.snd_nxt;
                    let rcv_nxt = conn.rcv_nxt;
                    drop(mgr);
                    send_tcp_segment(
                        dst_port,
                        src_ip,
                        src_port,
                        snd_nxt,
                        rcv_nxt,
                        TCP_FLAG_ACK,
                        &[],
                    );
                    return;
                }
            }
            TcpState::Established => {
                // Data delivery
                if !data.is_empty() {
                    conn.rx_buf.extend(data.iter().copied());
                    conn.rcv_nxt = conn.rcv_nxt.wrapping_add(data.len() as u32);
                    let (snd_nxt, rcv_nxt, lp) = (conn.snd_nxt, conn.rcv_nxt, conn.key.local_port);
                    drop(mgr);
                    send_tcp_segment(lp, src_ip, src_port, snd_nxt, rcv_nxt, TCP_FLAG_ACK, &[]);
                    return;
                }
                // FIN from peer
                if flags & TCP_FLAG_FIN != 0 {
                    conn.state = TcpState::CloseWait;
                    conn.rcv_nxt = conn.rcv_nxt.wrapping_add(1);
                    let (snd_nxt, rcv_nxt, lp) = (conn.snd_nxt, conn.rcv_nxt, conn.key.local_port);
                    drop(mgr);
                    send_tcp_segment(lp, src_ip, src_port, snd_nxt, rcv_nxt, TCP_FLAG_ACK, &[]);
                    return;
                }
            }
            TcpState::FinWait1 => {
                if flags & TCP_FLAG_ACK != 0 {
                    conn.state = TcpState::FinWait2;
                }
            }
            TcpState::FinWait2 => {
                if flags & TCP_FLAG_FIN != 0 {
                    conn.state = TcpState::Closed;
                    conn.rcv_nxt = conn.rcv_nxt.wrapping_add(1);
                    let (snd_nxt, rcv_nxt, lp) = (conn.snd_nxt, conn.rcv_nxt, conn.key.local_port);
                    drop(mgr);
                    send_tcp_segment(lp, src_ip, src_port, snd_nxt, rcv_nxt, TCP_FLAG_ACK, &[]);
                    return;
                }
            }
            TcpState::LastAck => {
                if flags & TCP_FLAG_ACK != 0 {
                    conn.state = TcpState::Closed;
                }
            }
            _ => {}
        }
    }
}

// ────────────────────────────────────────────────────────────
// Public API used by syscalls
// ────────────────────────────────────────────────────────────

/// Active open: sends SYN, returns Ok once Established (blocking poll).
pub fn tcp_connect(local_port: u16, dst_ip: [u8; 4], dst_port: u16) -> Result<(), &'static str> {
    let key = ConnKey {
        local_port,
        remote_ip: dst_ip,
        remote_port: dst_port,
    };
    let isn = pseudo_random_isn(dst_ip, local_port, dst_port);
    {
        let mut mgr = TCP_MANAGER.lock();
        mgr.connections.insert(
            key,
            TcpConnection {
                state: TcpState::SynSent,
                key,
                snd_nxt: isn.wrapping_add(1),
                rcv_nxt: 0,
                rx_buf: VecDeque::new(),
                socket_id: 0,
            },
        );
    }
    send_tcp_segment(local_port, dst_ip, dst_port, isn, 0, TCP_FLAG_SYN, &[]);

    // Poll until established or timeout
    for i in 0..50000 {
        crate::drivers::network::virtio::poll_rx();
        let mgr = TCP_MANAGER.lock();
        if let Some(conn) = mgr.connections.get(&key) {
            if conn.state == TcpState::Established {
                crate::debugln!("tcp_connect: Connection Established!");
                return Ok(());
            }
            if i % 10000 == 0 {
                crate::debugln!("tcp_connect polling: state={:?}", conn.state);
            }
        }
        drop(mgr);
        // small delay — spin
        for _ in 0..1000 {
            unsafe {
                core::arch::asm!("pause");
            }
        }
    }
    Err("connect timeout")
}

/// Passive open: marks a local port as listening.
pub fn tcp_listen(local_port: u16, socket_id: usize) {
    let mut mgr = TCP_MANAGER.lock();
    mgr.listeners.insert(local_port, socket_id);
    mgr.accept_queue
        .entry(socket_id)
        .or_insert_with(VecDeque::new);
}

/// Accept a pending connection. Returns ConnKey on success.
pub fn tcp_accept(socket_id: usize) -> Option<ConnKey> {
    let mut mgr = TCP_MANAGER.lock();
    mgr.accept_queue.get_mut(&socket_id)?.pop_front()
}

/// Send data on an established connection.
pub fn tcp_send(key: ConnKey, data: &[u8]) -> Result<usize, &'static str> {
    let snd_nxt;
    let rcv_nxt;
    {
        let mut mgr = TCP_MANAGER.lock();
        let conn = mgr.connections.get_mut(&key).ok_or("no connection")?;
        if conn.state != TcpState::Established {
            return Err("not established");
        }
        snd_nxt = conn.snd_nxt;
        rcv_nxt = conn.rcv_nxt;
        conn.snd_nxt = conn.snd_nxt.wrapping_add(data.len() as u32);
    }
    send_tcp_segment(
        key.local_port,
        key.remote_ip,
        key.remote_port,
        snd_nxt,
        rcv_nxt,
        TCP_FLAG_ACK | TCP_FLAG_PSH,
        data,
    );
    Ok(data.len())
}

/// Receive data. Returns bytes copied, or 0 if no data yet.
pub fn tcp_recv(key: ConnKey, buf: &mut [u8]) -> usize {
    crate::drivers::network::virtio::poll_rx();
    let mut mgr = TCP_MANAGER.lock();
    if let Some(conn) = mgr.connections.get_mut(&key) {
        let n = core::cmp::min(buf.len(), conn.rx_buf.len());
        for i in 0..n {
            buf[i] = conn.rx_buf.pop_front().unwrap();
        }
        n
    } else {
        0
    }
}

/// Close a connection: send FIN.
pub fn tcp_close(key: ConnKey) {
    let state_info;
    {
        let mut mgr = TCP_MANAGER.lock();
        if let Some(conn) = mgr.connections.get_mut(&key) {
            state_info = Some((conn.snd_nxt, conn.rcv_nxt));
            conn.state = TcpState::FinWait1;
        } else {
            return;
        }
    }
    if let Some((snd_nxt, rcv_nxt)) = state_info {
        send_tcp_segment(
            key.local_port,
            key.remote_ip,
            key.remote_port,
            snd_nxt,
            rcv_nxt,
            TCP_FLAG_FIN | TCP_FLAG_ACK,
            &[],
        );
    }
}

fn pseudo_random_isn(ip: [u8; 4], src_port: u16, dst_port: u16) -> u32 {
    // Simple deterministic ISN — good enough for a single-host OS
    let mut v: u32 = 0xA5A5A5A5;
    v ^= (ip[0] as u32) << 24 | (ip[1] as u32) << 16 | (ip[2] as u32) << 8 | ip[3] as u32;
    v ^= (src_port as u32) << 16 | dst_port as u32;
    v = v.wrapping_mul(0x9e3779b9);
    v ^= v >> 16;
    v
}

/// Pack a ConnKey into a u64 for storage without cross-module type imports.
/// Layout: [local_port(16) | remote_port(16) | remote_ip[0](8) | [1](8) | [2](8) | [3](8)]
pub fn conn_key_pack(key: ConnKey) -> u64 {
    ((key.local_port as u64) << 48)
        | ((key.remote_port as u64) << 32)
        | ((key.remote_ip[0] as u64) << 24)
        | ((key.remote_ip[1] as u64) << 16)
        | ((key.remote_ip[2] as u64) << 8)
        | (key.remote_ip[3] as u64)
}

/// Unpack a u64 back into a ConnKey.
pub fn conn_key_unpack(v: u64) -> ConnKey {
    ConnKey {
        local_port: ((v >> 48) & 0xFFFF) as u16,
        remote_port: ((v >> 32) & 0xFFFF) as u16,
        remote_ip: [
            ((v >> 24) & 0xFF) as u8,
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
        ],
    }
}
