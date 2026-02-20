use crate::interrupts::task::CPUState;

pub fn handle_net_send(context: &mut CPUState) {
    let ptr = context.rdi as *const u8;
    let len = context.rsi as usize;

    if ptr.is_null() || len == 0 {
        context.rax = 1;
        return;
    }

    let data = unsafe { core::slice::from_raw_parts(ptr, len) };
    
    // Enable interrupts to allow keyboard to work
    unsafe { core::arch::asm!("sti"); }
    
    let res = crate::drivers::network::virtio::send_packet(data);
    
    unsafe { core::arch::asm!("cli"); }
    
    context.rax = res as u64;
}

pub fn handle_net_recv(context: &mut CPUState) {
    let ptr = context.rdi as *mut u8;
    let len = context.rsi as usize;

    if ptr.is_null() || len == 0 {
        context.rax = 0;
        return;
    }

    // Enable interrupts to allow keyboard to work
    unsafe { core::arch::asm!("sti"); }

    let packet_opt = crate::drivers::network::virtio::recv_packet();
    
    unsafe { core::arch::asm!("cli"); }

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
    let domain = context.rdi; // 2 = AF_INET
    let type_ = context.rsi;  // 2 = SOCK_DGRAM, 1 = SOCK_STREAM
    let _proto = context.rdx;

    if domain != 2 || type_ != 2 {
        // Only IPv4 UDP supported for now
        context.rax = u64::MAX;
        return;
    }

    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        let pid = tm.tasks[current].as_ref().unwrap().process.as_ref().unwrap().pid;
        
        let socket_id = crate::net::socket::SOCKET_MANAGER.lock()
            .create_socket(pid, crate::net::socket::SocketType::Udp);
        
        if let Some(thread) = tm.tasks[current].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut socket_table = proc.socket_table.lock();
            for i in 0..16 {
                if socket_table[i].is_none() {
                    socket_table[i] = Some(socket_id);
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
    let _addr_len = context.rdx;

    if addr_ptr.is_null() {
        context.rax = u64::MAX;
        return;
    }

    if fd >= 16 {
        context.rax = u64::MAX;
        return;
    }

    let port = unsafe {
        // sockaddr_in: family(2), port(2), addr(4), zero(8)
        let p_n = *(addr_ptr.add(2) as *const u16);
        u16::from_be(p_n)
    };

    let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_ref() {
            let proc = thread.process.as_ref().unwrap();
            let socket_id_opt = proc.socket_table.lock()[fd];
            
            if let Some(socket_id) = socket_id_opt {
                drop(tm);
                
                let res = crate::net::socket::SOCKET_MANAGER.lock().bind(socket_id, port);
                context.rax = if res.is_ok() { 0 } else { u64::MAX };
                return;
            }
        }
    }
    context.rax = u64::MAX;
}

pub fn handle_sendto(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *const u8;
    let len = context.rdx as usize;
    let _flags = context.r10;
    let dest_addr_ptr = context.r8 as *const u8;
    let _dest_len = context.r9;

    if buf_ptr.is_null() || dest_addr_ptr.is_null() || fd >= 16 {
        context.rax = u64::MAX;
        return;
    }

    let payload = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
    
    let (dst_ip, dst_port) = unsafe {
        let p_n = *(dest_addr_ptr.add(2) as *const u16);
        let port = u16::from_be(p_n);
        let ip_ptr = dest_addr_ptr.add(4);
        let ip = [ *ip_ptr, *ip_ptr.add(1), *ip_ptr.add(2), *ip_ptr.add(3) ];
        (ip, port)
    };

    let mut socket_id = 0;
    let mut src_port = 0;

    {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks[current].as_ref() {
                let proc = thread.process.as_ref().unwrap();
                if let Some(sid) = proc.socket_table.lock()[fd] {
                    socket_id = sid;
                }
            }
        }
    }

    if socket_id != 0 {
        let sm = crate::net::socket::SOCKET_MANAGER.lock();
        if let Some(socket) = sm.sockets.get(&socket_id) {
            src_port = socket.local_port;
        }
    }

    if src_port == 0 {
        // Ephemeral port hack
        src_port = 49152 + (socket_id as u16 % 16384);
        // Should really update the socket state
    }

    // Enable interrupts for virtio send
    unsafe { core::arch::asm!("sti"); }
    
    crate::net::udp::send_udp(src_port, dst_ip, dst_port, payload);
    
    unsafe { core::arch::asm!("cli"); }

    context.rax = len as u64;
}

pub fn handle_recvfrom(context: &mut CPUState) {
    let fd = context.rdi as usize;
    let buf_ptr = context.rsi as *mut u8;
    let len = context.rdx as usize;
    let _flags = context.r10;
    let src_addr_ptr = context.r8 as *mut u8;
    let addr_len_ptr = context.r9 as *mut u32;

    if buf_ptr.is_null() || fd >= 16 {
        context.rax = u64::MAX;
        return;
    }

    let mut socket_id = 0;
    {
        let tm = crate::interrupts::task::TASK_MANAGER.int_lock();
        if let Some(current) = tm.current_task_idx() {
            if let Some(thread) = tm.tasks[current].as_ref() {
                let proc = thread.process.as_ref().unwrap();
                if let Some(sid) = proc.socket_table.lock()[fd] {
                    socket_id = sid;
                }
            }
        }
    }

    if socket_id == 0 {
        context.rax = u64::MAX;
        return;
    }

    // Enable interrupts to poll
    unsafe { core::arch::asm!("sti"); }
    
    // Trigger a poll/pull from virtio just in case
    crate::drivers::network::virtio::poll_rx();
    
    unsafe { core::arch::asm!("cli"); }

    let mut sm = crate::net::socket::SOCKET_MANAGER.lock();
    if let Some(packet) = sm.pop_packet(socket_id) {
        // Packet format in queue: SrcIP(4) + SrcPort(2) + Payload
        if packet.len() >= 6 {
            let src_ip = &packet[0..4];
            let src_port_be = &packet[4..6];
            let payload = &packet[6..];
            
            let copy_len = core::cmp::min(len, payload.len());
            unsafe {
                core::ptr::copy_nonoverlapping(payload.as_ptr(), buf_ptr, copy_len);
                
                if !src_addr_ptr.is_null() {
                    // Fill sockaddr_in
                    *(src_addr_ptr as *mut u16) = 2; // AF_INET
                    let port_ptr = src_addr_ptr.add(2);
                    *port_ptr = src_port_be[0];
                    *port_ptr.add(1) = src_port_be[1];
                    
                    let ip_ptr = src_addr_ptr.add(4);
                    core::ptr::copy_nonoverlapping(src_ip.as_ptr(), ip_ptr, 4);
                }
                
                if !addr_len_ptr.is_null() {
                    *addr_len_ptr = 16;
                }
            }
            context.rax = copy_len as u64;
        } else {
            context.rax = 0;
        }
    } else {
        context.rax = u64::MAX; // Error/Empty
    }
}

pub fn handle_close_socket(context: &mut CPUState) {
    let handle = context.rdi as usize;
    let mut tm = crate::interrupts::task::TASK_MANAGER.int_lock();
    if let Some(current) = tm.current_task_idx() {
        if let Some(thread) = tm.tasks[current].as_mut() {
            let proc = thread.process.as_ref().expect("Thread has no process");
            let mut socket_table = proc.socket_table.lock();
            if handle < 16 {
                socket_table[handle] = None;
                context.rax = 0;
                return;
            }
        }
    }
    context.rax = u64::MAX;
}