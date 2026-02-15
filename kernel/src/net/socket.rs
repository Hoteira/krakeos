use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use crate::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    Udp,
    Tcp,
    Raw,
}

pub struct Socket {
    pub id: usize,
    pub socket_type: SocketType,
    pub local_port: u16,
    pub pid: u64,
    pub rx_queue: Vec<Vec<u8>>, // Simple queue of packets (payloads)
}

pub struct SocketManager {
    pub sockets: BTreeMap<usize, Socket>,
    pub udp_bindings: BTreeMap<u16, usize>, // Port -> SocketID
    next_id: usize,
}

pub static SOCKET_MANAGER: Mutex<SocketManager> = Mutex::new(SocketManager {
    sockets: BTreeMap::new(),
    udp_bindings: BTreeMap::new(),
    next_id: 1,
});

impl SocketManager {
    pub fn create_socket(&mut self, pid: u64, socket_type: SocketType) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        
        let socket = Socket {
            id,
            socket_type,
            local_port: 0,
            pid,
            rx_queue: Vec::new(),
        };
        
        self.sockets.insert(id, socket);
        id
    }

    pub fn bind(&mut self, id: usize, port: u16) -> Result<(), &'static str> {
        if let Some(socket) = self.sockets.get_mut(&id) {
            if socket.local_port != 0 {
                return Err("Socket already bound");
            }
            
            if self.udp_bindings.contains_key(&port) {
                return Err("Port already in use");
            }
            
            socket.local_port = port;
            self.udp_bindings.insert(port, id);
            Ok(())
        } else {
            Err("Socket not found")
        }
    }

    pub fn push_packet(&mut self, port: u16, packet: Vec<u8>) {
        if let Some(&socket_id) = self.udp_bindings.get(&port) {
            if let Some(socket) = self.sockets.get_mut(&socket_id) {
                socket.rx_queue.push(packet);
                // TODO: Signal/Wakeup process
            }
        }
    }
    
    pub fn pop_packet(&mut self, id: usize) -> Option<Vec<u8>> {
        if let Some(socket) = self.sockets.get_mut(&id) {
            if !socket.rx_queue.is_empty() {
                return Some(socket.rx_queue.remove(0));
            }
        }
        None
    }
}
