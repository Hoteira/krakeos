#![no_std]

extern crate alloc;

use alloc::string::String;
use std::net::socket::{Socket, SocketAddr};

pub fn main() {
    std::debugln!("NET_TEST: Starting UDP Loopback Test (New Socket API)...");

    // 1. Create Server Socket
    let server = match Socket::new() {
        Some(s) => s,
        None => {
            std::debugln!("NET_TEST: Failed to create server socket");
            return;
        }
    };

    let bind_addr = SocketAddr::V4([0, 0, 0, 0], 8888);
    std::debugln!("NET_TEST: Binding to {:?}...", bind_addr);
    if let Err(e) = server.bind(bind_addr) {
        std::debugln!("NET_TEST: Bind failed with error {}", e);
        return;
    }

    // 2. Create Client Socket
    let client = match Socket::new() {
        Some(s) => s,
        None => {
            std::debugln!("NET_TEST: Failed to create client socket");
            return;
        }
    };

    let dest_addr = SocketAddr::V4([127, 0, 0, 1], 8888);
    let msg = "Hello from Client!";

    // 3. Send Data
    std::debugln!("NET_TEST: Sending '{}' to {:?}...", msg, dest_addr);
    match client.send_to(msg.as_bytes(), dest_addr) {
        Ok(n) => std::debugln!("NET_TEST: Sent {} bytes", n),
        Err(e) => {
            std::debugln!("NET_TEST: Send failed with error {}", e);
            return;
        }
    }

    // 4. Recv Data
    std::debugln!("NET_TEST: Receiving...");
    let mut buf = [0u8; 128];
    
    // Poll for a bit
    for _ in 0..1000 {
        match server.recv_from(&mut buf) {
            Ok((n, src)) => {
                 if n > 0 {
                     let s = String::from_utf8_lossy(&buf[..n]);
                     std::debugln!("NET_TEST: Received '{}' from {:?}", s, src);
                     
                     if s == msg {
                         std::debugln!("NET_TEST: SUCCESS! Data matches.");
                         return;
                     } else {
                         std::debugln!("NET_TEST: FAILURE! Data mismatch.");
                         return;
                     }
                 }
            }
            Err(-1) => {
                // No data, continue polling
            }
            Err(e) => {
                std::debugln!("NET_TEST: Recv error: {}", e);
                return;
            }
        }
        
        // Basic delay/yield
        std::sys::yield_task();
    }

    std::debugln!("NET_TEST: Timed out waiting for data.");
}