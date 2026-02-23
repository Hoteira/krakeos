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
                        break;
                    } else {
                        std::debugln!("NET_TEST: FAILURE! Data mismatch.");
                        break;
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

        std::sys::yield_task();
    }

    std::debugln!("NET_TEST: Timed out waiting for UDP data.");

    std::debugln!("=====================================================");
    std::debugln!("NET_TEST: Starting TCP Loopback Test...");

    // 1. Setup Server
    let listener = match std::net::TcpListener::bind(8080) {
        Some(l) => {
            std::debugln!("NET_TEST: TCP Listener bound to 8080");
            l
        }
        None => {
            std::debugln!("NET_TEST: Failed to bind TCP listener");
            return;
        }
    };

    // 2. Setup Client & Connect
    let mut client = match std::net::TcpStream::connect([127, 0, 0, 1], 8080) {
        Some(s) => {
            std::debugln!("NET_TEST: TCP Client connected to 127.0.0.1:8080");
            s
        }
        None => {
            std::debugln!("NET_TEST: Failed to connect TCP client");
            return;
        }
    };

    // 3. Server Accept
    let mut server_conn = match listener.accept() {
        Some(s) => {
            std::debugln!("NET_TEST: TCP Server accepted connection");
            s
        }
        None => {
            std::debugln!("NET_TEST: Failed to accept TCP connection");
            return;
        }
    };

    // 4. Send Client -> Server
    let tcp_msg = "Hello TCP World!";
    std::debugln!("NET_TEST: Client sending: '{}'", tcp_msg);
    match client.write_all(tcp_msg.as_bytes()) {
        Ok(n) => std::debugln!("NET_TEST: Client sent {} bytes", n),
        Err(e) => {
            std::debugln!("NET_TEST: Client send error: {}", e);
            return;
        }
    }

    // 5. Recv on Server
    let mut tcp_buf = [0u8; 128];
    // Loopback is synchronous but we might need to yield once or twice just in case
    let mut recv_len = 0;
    for _ in 0..100 {
        match server_conn.read(&mut tcp_buf) {
            Ok(n) if n > 0 => {
                recv_len = n;
                break;
            }
            _ => std::sys::yield_task(),
        }
    }

    if recv_len > 0 {
        let s = String::from_utf8_lossy(&tcp_buf[..recv_len]);
        std::debugln!("NET_TEST: Server received: '{}'", s);
        if s == tcp_msg {
            std::debugln!("NET_TEST: TCP SUCCESS! Data matches.");
        } else {
            std::debugln!("NET_TEST: TCP FAILURE! Data mismatch.");
        }
    } else {
        std::debugln!("NET_TEST: TCP Timed out waiting for data on server.");
    }
}
