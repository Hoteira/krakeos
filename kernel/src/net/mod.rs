pub mod arp;
pub mod icmp;
pub mod ipv4;
pub mod socket;
pub mod tcp;
pub mod udp;

// QEMU/VirtIO Defaults
pub static mut LOCAL_IP: [u8; 4] = [10, 0, 2, 15];
pub static mut LOCAL_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

use crate::sync::Mutex;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

pub static LOOPBACK_QUEUE: Mutex<VecDeque<Vec<u8>>> = Mutex::new(VecDeque::new());

pub fn push_loopback_packet(packet: Vec<u8>) {
    LOOPBACK_QUEUE.lock().push_back(packet);
}

use core::sync::atomic::{AtomicBool, Ordering};
static IN_POLL: AtomicBool = AtomicBool::new(false);

static mut LOOPBACK_DEPTH: u32 = 0;

pub fn poll_loopback() {
    if IN_POLL.swap(true, Ordering::SeqCst) {
        return;
    }

    let flags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            out(reg) flags
        );
        core::arch::asm!("cli");
    }

    let mut processed = 0;
    // Limit packets per poll to prevent infinite loop or stack stress
    loop {
        let packet_opt = {
            let mut q = LOOPBACK_QUEUE.lock();
            q.pop_front()
        };
        if let Some(packet) = packet_opt {
            unsafe { LOOPBACK_DEPTH += 1; }
            crate::net::on_receive(&packet);
            unsafe { LOOPBACK_DEPTH -= 1; }
            processed += 1;
            if processed >= 32 { break; }
        } else {
            break;
        }
    }

    if flags & 0x200 != 0 {
        unsafe {
            core::arch::asm!("sti");
        }
    }
    
    IN_POLL.store(false, Ordering::SeqCst);
}

pub fn on_receive(packet: &[u8]) {
    if packet.len() < 14 {
        return;
    }

    let eth_type = ((packet[12] as u16) << 8) | packet[13] as u16;
    let src_mac = [
        packet[6], packet[7], packet[8], packet[9], packet[10], packet[11],
    ];

    match eth_type {
        0x0806 => {
            arp::handle_arp(packet);
        }
        0x0800 => {
            ipv4::handle_ipv4(&packet[14..], src_mac);
        }
        _ => {}
    }
}
