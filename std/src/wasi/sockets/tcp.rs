#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "wasi:sockets/tcp@0.2.0")]
unsafe extern "C" {
    #[link_name = "[method]tcp-socket.start-bind"]
    pub fn start_bind(socket: i32, network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.finish-bind"]
    pub fn finish_bind(socket: i32, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.start-connect"]
    pub fn start_connect(socket: i32, network: i32, ip_addr_ptr: *const u8, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.finish-connect"]
    pub fn finish_connect(socket: i32, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.start-listen"]
    pub fn start_listen(socket: i32, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.finish-listen"]
    pub fn finish_listen(socket: i32, result_ptr: *mut u8);

    #[link_name = "[method]tcp-socket.accept"]
    pub fn accept(socket: i32, result_ptr: *mut u8);
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_bind(_socket: i32, _network: i32, _ip_addr_ptr: *const u8, result_ptr: *mut u8) {
    *result_ptr = 1; // Not supported natively yet
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_bind(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 1;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_connect(_socket: i32, _network: i32, _ip_addr_ptr: *const u8, result_ptr: *mut u8) {
    *result_ptr = 1;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_connect(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 1;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn start_listen(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 1;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn finish_listen(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 1;
}

#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn accept(_socket: i32, result_ptr: *mut u8) {
    *result_ptr = 1;
}