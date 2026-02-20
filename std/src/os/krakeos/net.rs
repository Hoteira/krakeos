use crate::wasi::krakeos;

pub fn send(packet: &[u8]) -> i32 {
    unsafe {
        krakeos::krakeos_net_send(packet.as_ptr(), packet.len() as u32)
    }
}

pub fn recv(packet: &mut [u8]) -> usize {
    unsafe {
        let res = krakeos::krakeos_net_recv(packet.as_mut_ptr(), packet.len() as u32);
        if res < 0 {
            0
        } else {
            res as usize
        }
    }
}
