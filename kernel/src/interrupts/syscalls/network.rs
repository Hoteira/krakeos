use crate::interrupts::task::CPUState;

pub fn handle_net_send(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;

    if ptr.is_null() || len == 0 {
        context.rax = 1;
        return;
    }
    if !super::validate_user_buf(context, ptr as u64, len as u64) { return; }

    let data = unsafe { core::slice::from_raw_parts(ptr, len) };
    let res = crate::drivers::network::virtio::send_packet(data);
    context.rax = res as u64;
}

pub fn handle_net_recv(context: &mut CPUState) {
    let ptr = context.rdi as *mut u8;
    let len = context.rsi as usize;

    if ptr.is_null() || len == 0 {
        context.rax = 0;
        return;
    }
    if !super::validate_user_buf(context, ptr as u64, len as u64) { return; }

    let packet_opt = crate::drivers::network::virtio::recv_packet();

    if let Some(packet) = packet_opt {
        let copy_len = core::cmp::min(len, packet.len());
        unsafe {
            core::ptr::copy_nonoverlapping(packet.as_ptr(), ptr, copy_len);
        }
        context.rax = copy_len as u64;
    } else {
        context.rax = 0;
    }
}

pub fn handle_socket(context: &mut CPUState) {
    let domain = context.rdi;
    let type_ = context.rsi;
    let proto = context.rdx;

    if domain != 2 || (type_ != 1 && type_ != 2) {
        context.rax = u64::MAX;
        return;
    }

    let socket_kind = if type_ == 1 {
        crate::net::socket::SocketType::Tcp
    } else {
        crate::net::socket::SocketType::Udp
    };

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        let pid = tm.tasks.get(&(current)).unwrap().process.as_ref().unwrap().pid;
        let socket_id = crate::net::socket::SOCKET_MANAGER.lock().create_socket(pid, socket_kind);
        
        if let Some(thread) = tm.tasks.get_mut(&(current)) {
            let proc = thread.process.as_ref().unwrap();
            let mut table = proc.socket_table.lock();
            for i in 0..16 {
                if table[i].is_none() {
                    table[i] = Some(socket_id);
                    context.rax = i as u64;
                    return;
                }
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_bind(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let addr_ptr = context.rsi as *const u8;

    if addr_ptr.is_null() || fd >= 16 {
        context.rax = u64::MAX;
        return;
    }
    if !super::validate_user_buf(context, addr_ptr as u64, 8) { return; }

    let port = unsafe { u16::from_be(*(addr_ptr.add(2) as *const u16)) };

    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    let res = crate::net::socket::SOCKET_MANAGER.lock().bind(socket_id, port);
    let ret = if res.is_ok() { 0 } else { u64::MAX };
    context.rax = ret;
}

pub fn handle_connect(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let addr_ptr = context.rsi as *const u8;

    if addr_ptr.is_null() || fd >= 16 {
        context.rax = u64::MAX;
        return;
    }
    if !super::validate_user_buf(context, addr_ptr as u64, 8) { return; }

    let (dst_ip, dst_port) = unsafe {
        let port = u16::from_be_bytes([*addr_ptr.add(2), *addr_ptr.add(3)]);
        let ip = [*addr_ptr.add(4), *addr_ptr.add(5), *addr_ptr.add(6), *addr_ptr.add(7)];
        (ip, port)
    };

    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    let local_port = 49152 + (socket_id as u16 % 16384);
    let res = crate::net::tcp::tcp_connect_start(local_port, dst_ip, dst_port, socket_id);
    context.rax = if let Ok(key) = res {
        crate::net::socket::SOCKET_MANAGER.lock().tcp_connections.insert(socket_id, crate::net::tcp::conn_key_pack(key));
        0
    } else {
        u64::MAX
    };
}

pub fn handle_connect_finish(context: &mut CPUState) {
    let fd = context.rdi as usize;
    
    // Poll loopback/physical queue to process SYN-ACK or other handshake steps
    crate::drivers::network::virtio::poll_rx();

    if fd >= 16 {
        context.rax = u64::MAX;
        return;
    }

    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    let key_packed = crate::net::socket::SOCKET_MANAGER.lock().tcp_connections.get(&socket_id).copied();
    if let Some(packed) = key_packed {
        let key = crate::net::tcp::conn_key_unpack(packed);
        let connected = crate::net::tcp::tcp_connect_check(key);
        let ret = if connected { 0 } else { 1 };
        context.rax = ret as u64;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_listen(context: &mut CPUState) {
    let fd = context.rdi as usize;
    
    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    let local_port = crate::net::socket::SOCKET_MANAGER.lock().sockets.get(&socket_id).map(|s| s.local_port).unwrap_or(0);
    if local_port == 0 {
        context.rax = u64::MAX;
        return;
    }

    crate::net::tcp::tcp_listen(local_port, socket_id);
    context.rax = 0;
}

pub fn handle_accept(context: &mut CPUState) {
    let fd = context.rdi as usize;
    
    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    crate::drivers::network::virtio::poll_rx();
    if let Some(key) = crate::net::tcp::tcp_accept(socket_id) {
        let new_sid = crate::net::socket::SOCKET_MANAGER.lock().create_socket(0, crate::net::socket::SocketType::Tcp);
        {
            let mut sm = crate::net::socket::SOCKET_MANAGER.lock();
            if let Some(sock) = sm.sockets.get_mut(&new_sid) { sock.local_port = key.local_port; }
            sm.tcp_connections.insert(new_sid, crate::net::tcp::conn_key_pack(key));
            if let Some(conn) = crate::net::tcp::TCP_MANAGER.int_lock().connections.get_mut(&key) {
                conn.socket_id = new_sid;
            }
        }
        
        let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            let proc = tm.tasks.get(&(current)).unwrap().process.as_ref().unwrap();
            let mut table = proc.socket_table.lock();
            for i in 0..16 {
                if table[i].is_none() {
                    table[i] = Some(new_sid);
                    context.rax = i as u64;
                    return;
                }
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_tcp_send(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;

    if !super::validate_user_buf(context, buf_ptr as u64, len as u64) { return; }

    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    let key_packed = crate::net::socket::SOCKET_MANAGER.lock().tcp_connections.get(&socket_id).copied();
    if let Some(packed) = key_packed {
        let key = crate::net::tcp::conn_key_unpack(packed);
        let data = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
        let res = crate::net::tcp::tcp_send(key, data);
        let ret = res.map(|n| n as u64).unwrap_or(u64::MAX);
        context.rax = ret;
    } else {
        context.rax = u64::MAX;
    }
}

pub fn handle_tcp_recv(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;

    if !super::validate_user_buf(context, buf_ptr as u64, len as u64) { return; }

    let socket_id = match get_socket_id(context, fd) {
        Some(s) => s,
        None => {
            context.rax = u64::MAX;
            return;
        }
    };

    crate::drivers::network::virtio::poll_rx();
    let mut sm = crate::net::socket::SOCKET_MANAGER.lock();
    if let Some(packet) = sm.pop_packet(socket_id) {
        let copy_len = core::cmp::min(len, packet.len());
        unsafe { core::ptr::copy_nonoverlapping(packet.as_ptr(), buf_ptr, copy_len); }
        context.rax = copy_len as u64;
    } else {
        context.rax = 0;
    }
}

pub fn handle_close_socket(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        let proc = tm.tasks.get(&(current)).unwrap().process.as_ref().unwrap();
        let mut table = proc.socket_table.lock();
        if fd < 16 {
            table[fd] = None;
            context.rax = 0;
            return;
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_sendto(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;
    let dest_addr_ptr = context.r8 as *const u8;

    if !super::validate_user_buf(context, buf_ptr as u64, len as u64) { return; }
    if !super::validate_user_buf(context, dest_addr_ptr as u64, 8) { return; }

    let (dst_ip, dst_port) = unsafe {
        let port = u16::from_be(*(dest_addr_ptr.add(2) as *const u16));
        let ip_ptr = dest_addr_ptr.add(4);
        ([*ip_ptr, *ip_ptr.add(1), *ip_ptr.add(2), *ip_ptr.add(3)], port)
    };

    let socket_id = match get_socket_id(context, fd) { Some(s) => s, None => 0 };
    let mut src_port = 0;
    if socket_id != 0 {
        src_port = crate::net::socket::SOCKET_MANAGER.lock().sockets.get(&socket_id).map(|s| s.local_port).unwrap_or(0);
    }
    if src_port == 0 { src_port = 49152 + (socket_id as u16 % 16384); }

    let data = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
    crate::net::udp::send_udp(src_port, dst_ip, dst_port, data);
    context.rax = len as u64;
}

pub fn handle_recvfrom(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    let src_addr_ptr = context.r8 as *mut u8;
    let addr_len_ptr = context.r9 as *mut u32;

    if !super::validate_user_buf(context, buf_ptr as u64, len as u64) { return; }
    if !src_addr_ptr.is_null() && !super::validate_user_buf(context, src_addr_ptr as u64, 8) { return; }
    if !addr_len_ptr.is_null() && !super::validate_user_buf(context, addr_len_ptr as u64, 4) { return; }

    let socket_id = match get_socket_id(context, fd) { Some(s) => s, None => 0 };
    if socket_id == 0 { context.rax = u64::MAX; return; }

    crate::drivers::network::virtio::poll_rx();
    let mut sm = crate::net::socket::SOCKET_MANAGER.lock();
    if let Some(packet) = sm.pop_packet(socket_id) {
        if packet.len() >= 6 {
            let src_ip = &packet[0..4];
            let src_port_be = &packet[4..6];
            let payload = &packet[6..];
            let copy_len = core::cmp::min(len, payload.len());
            unsafe {
                core::ptr::copy_nonoverlapping(payload.as_ptr(), buf_ptr, copy_len);
                if !src_addr_ptr.is_null() {
                    *(src_addr_ptr as *mut u16) = 2;
                    core::ptr::copy_nonoverlapping(src_port_be.as_ptr(), src_addr_ptr.add(2), 2);
                    core::ptr::copy_nonoverlapping(src_ip.as_ptr(), src_addr_ptr.add(4), 4);
                }
                if !addr_len_ptr.is_null() { *addr_len_ptr = 16; }
            }
            context.rax = copy_len as u64;
            return;
        }
    }
    context.rax = u64::MAX;
}

fn get_socket_id(context: &CPUState, fd: usize) -> Option<usize> {
    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    let current = tm.current_task_idx()?;
    let proc = tm.tasks.get(&(current))?.process.as_ref()?;
    proc.socket_table.lock()[fd]
}
