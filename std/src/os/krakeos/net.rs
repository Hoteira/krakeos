pub fn send(packet: &[u8]) -> i32 {
    let res = unsafe {
        crate::os::net_send(packet.as_ptr(), packet.len() as u32)
    };
    res
}

pub fn recv(packet: &mut [u8]) -> usize {
    let res = unsafe {
        crate::os::net_recv(packet.as_mut_ptr(), packet.len() as u32)
    };
    let ret = if res < 0 {
        0
    } else {
        res as usize
    };
    ret
}
