pub fn send(packet: &[u8]) -> i32 {
    crate::debugln!("CALLING TCP FN net::send WITH ARGS: len={}", packet.len());
    let res = unsafe {
        crate::os::net_send(packet.as_ptr(), packet.len() as u32)
    };
    crate::debugln!("TCP RESULT: net::send RESULT: {}", res);
    res
}

pub fn recv(packet: &mut [u8]) -> usize {
    crate::debugln!("CALLING TCP FN net::recv WITH ARGS: max_len={}", packet.len());
    let res = unsafe {
        crate::os::net_recv(packet.as_mut_ptr(), packet.len() as u32)
    };
    let ret = if res < 0 {
        0
    } else {
        res as usize
    };
    crate::debugln!("TCP RESULT: net::recv RESULT: {}", ret);
    ret
}
