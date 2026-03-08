use crate::alloc::{string::String, vec, vec::Vec};
use crate::wasm::{
    common::{
        checked::{AbstractStored, Stored},
        config::Config,
        interop::Linker,
        reader::types::{FuncType, NumType, ResultType, ValType},
        value::Value,
    },
    interpreter::{
        resumable::RunState,
        store::{addrs::FuncAddr, ExternVal, HaltExecutionError, Store},
    },
};

pub fn create_wasi_p2_imports<T: Config + Clone + Send + 'static>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
    }

    // krakeos:system/container@0.1.0
    {
        let module = "krakeos:system/container@0.1.0";
        define(linker, store, module, "plant",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![],
            crate::os::krakeos::wasi::container_plant_host);        define(linker, store, module, "plant-from-path", 
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::container_plant_from_path_host);
        define(linker, store, module, "harvest", 
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::container_harvest_host);
        define(linker, store, module, "list-children", 
            vec![ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::container_list_children_host);
        define(linker, store, module, "kill-child", 
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::container_kill_child_host);
    }

    // krakeos:system/terminal@0.1.0
    {
        let module = "krakeos:system/terminal@0.1.0";
        define(linker, store, module, "set-window-size", 
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::terminal_set_window_size);
        define(linker, store, module, "get-window-size", 
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::terminal_get_window_size);
    }

    // krakeos:system/debug@0.1.0
    {
        let module = "krakeos:system/debug@0.1.0";
        define(linker, store, module, "get-process-list", 
            vec![ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::get_process_list_host);
        define(linker, store, module, "kill", 
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::kill_host);
        define(linker, store, module, "dump-vma", 
            vec![ValType::NumType(NumType::I32)], 
            vec![], 
            crate::os::krakeos::wasi::dump_vma_host);
        define(linker, store, module, "get-memory-usage", 
            vec![], 
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], 
            crate::os::krakeos::wasi::get_memory_usage_host);
    }

    // wasi:cli/terminal-input@0.2.0
    {
        let module = "wasi:cli/terminal-input@0.2.0";
        define(linker, store, module, "[resource-drop]terminal-input", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:cli/terminal-output@0.2.0
    {
        let module = "wasi:cli/terminal-output@0.2.0";
        define(linker, store, module, "[resource-drop]terminal-output", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:cli/terminal-stdin@0.2.0
    {
        let module = "wasi:cli/terminal-stdin@0.2.0";
        define(linker, store, module, "get-terminal-stdin", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stdin_host);
    }
    // wasi:cli/terminal-stdout@0.2.0
    {
        let module = "wasi:cli/terminal-stdout@0.2.0";
        define(linker, store, module, "get-terminal-stdout", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stdout_host);
    }
    // wasi:cli/terminal-stderr@0.2.0
    {
        let module = "wasi:cli/terminal-stderr@0.2.0";
        define(linker, store, module, "get-terminal-stderr", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stderr_host);
    }

    // wasi:cli/stdout@0.2.0
    {
        let module = "wasi:cli/stdout@0.2.0";
        define(linker, store, module, "get-stdout", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stdout_host);
    }
    // wasi:cli/stdin@0.2.0
    {
        let module = "wasi:cli/stdin@0.2.0";
        define(linker, store, module, "get-stdin", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stdin_host);
    }
    // wasi:cli/stderr@0.2.0
    {
        let module = "wasi:cli/stderr@0.2.0";
        define(linker, store, module, "get-stderr", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stderr_host);
    }
    // wasi:io/streams@0.2.0
    {
        let module = "wasi:io/streams@0.2.0";
        define(linker, store, module, "[method]output-stream.write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_write);
        define(linker, store, module, "[method]output-stream.blocking-write-and-flush", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_write);
        define(linker, store, module, "[method]input-stream.read", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_read);
        define(linker, store, module, "[method]input-stream.blocking-read", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_read);
        define(linker, store, module, "[method]input-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::io::wasi::input_stream_subscribe);
        define(linker, store, module, "[method]output-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::io::wasi::output_stream_subscribe);

        define(linker, store, module, "[method]input-stream.skip", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_skip);
        define(linker, store, module, "[method]input-stream.blocking-skip", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_skip);
        define(linker, store, module, "[method]output-stream.flush", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_flush);
        define(linker, store, module, "[method]output-stream.blocking-flush", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_flush);
        define(linker, store, module, "[method]output-stream.write-zeroes", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_write_zeroes);
        define(linker, store, module, "[method]output-stream.blocking-write-zeroes", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_write_zeroes);
        define(linker, store, module, "[method]output-stream.splice", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_splice);
        define(linker, store, module, "[method]output-stream.blocking-splice", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::stream_splice);

        define(linker, store, module, "[resource-drop]input-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]output-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:io/poll@0.2.0
    {
        let module = "wasi:io/poll@0.2.0";
        define(linker, store, module, "poll", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::poll_poll);
        define(linker, store, module, "[method]pollable.ready", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::io::wasi::poll_ready);
        define(linker, store, module, "[method]pollable.block", vec![ValType::NumType(NumType::I32)], vec![], crate::io::wasi::poll_block);
        define(linker, store, module, "[resource-drop]pollable", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:io/error@0.2.0
    {
        let module = "wasi:io/error@0.2.0";
        define(linker, store, module, "[resource-drop]error", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[method]error.to-debug-string", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::io::wasi::error_to_debug_string);
    }
    // wasi:sockets/udp@0.2.0
    {
        let module = "wasi:sockets/udp@0.2.0";
        define(linker, store, module, "[resource-drop]udp-socket", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]incoming-datagram-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]outgoing-datagram-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[method]udp-socket.create", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::create_udp_socket);
        define(linker, store, module, "[method]udp-socket.start-bind", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_start_bind);
        define(linker, store, module, "[method]udp-socket.finish-bind", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_finish_bind);
        define(linker, store, module, "[method]udp-socket.stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_stream);
        define(linker, store, module, "[method]udp-socket.local-address", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_local_address);
        define(linker, store, module, "[method]udp-socket.remote-address", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_remote_address);
        define(linker, store, module, "[method]udp-socket.address-family", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::udp_address_family);
        define(linker, store, module, "[method]udp-socket.unicast-hop-limit", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::udp_get_unicast_hop_limit);
        define(linker, store, module, "[method]udp-socket.set-unicast-hop-limit", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_set_unicast_hop_limit);
        define(linker, store, module, "[method]udp-socket.receive-buffer-size", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::udp_get_receive_buffer_size);
        define(linker, store, module, "[method]udp-socket.set-receive-buffer-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_set_receive_buffer_size);
        define(linker, store, module, "[method]udp-socket.send-buffer-size", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::udp_get_send_buffer_size);
        define(linker, store, module, "[method]udp-socket.set-send-buffer-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_set_send_buffer_size);
        define(linker, store, module, "[method]udp-socket.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::udp_subscribe);
        define(linker, store, module, "[method]incoming-datagram-stream.receive", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_receive);
        define(linker, store, module, "[method]incoming-datagram-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::udp_incoming_subscribe);
        define(linker, store, module, "[method]outgoing-datagram-stream.check-send", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_check_send);
        define(linker, store, module, "[method]outgoing-datagram-stream.send", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_send);
        define(linker, store, module, "[method]outgoing-datagram-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::udp_outgoing_subscribe);
    }
    // wasi:sockets/tcp@0.2.0
    {
        let module = "wasi:sockets/tcp@0.2.0";
        define(linker, store, module, "[resource-drop]tcp-socket", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[method]tcp-socket.create", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_create_socket);
        define(linker, store, module, "[method]tcp-socket.start-bind", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_start_bind);
        define(linker, store, module, "[method]tcp-socket.finish-bind", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_finish_bind);
        define(linker, store, module, "[method]tcp-socket.start-connect", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_start_connect);
        define(linker, store, module, "[method]tcp-socket.finish-connect", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_finish_connect);
        define(linker, store, module, "[method]tcp-socket.start-listen", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_start_listen);
        define(linker, store, module, "[method]tcp-socket.finish-listen", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_finish_listen);
        define(linker, store, module, "[method]tcp-socket.accept", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_accept);
        define(linker, store, module, "[method]tcp-socket.send", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_send);
        define(linker, store, module, "[method]tcp-socket.recv", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_recv);
        define(linker, store, module, "[method]tcp-socket.local-address", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_local_address);
        define(linker, store, module, "[method]tcp-socket.remote-address", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_remote_address);
        define(linker, store, module, "[method]tcp-socket.is-listening", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_is_listening);
        define(linker, store, module, "[method]tcp-socket.address-family", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_address_family);
        define(linker, store, module, "[method]tcp-socket.set-listen-backlog-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_listen_backlog_size);
        define(linker, store, module, "[method]tcp-socket.keep-alive-enabled", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_get_keep_alive_enabled);
        define(linker, store, module, "[method]tcp-socket.set-keep-alive-enabled", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_keep_alive_enabled);
        define(linker, store, module, "[method]tcp-socket.keep-alive-idle-time", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::tcp_get_keep_alive_idle_time);
        define(linker, store, module, "[method]tcp-socket.set-keep-alive-idle-time", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_keep_alive_idle_time);
        define(linker, store, module, "[method]tcp-socket.keep-alive-interval", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::tcp_get_keep_alive_interval);
        define(linker, store, module, "[method]tcp-socket.set-keep-alive-interval", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_keep_alive_interval);
        define(linker, store, module, "[method]tcp-socket.keep-alive-count", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_get_keep_alive_count);
        define(linker, store, module, "[method]tcp-socket.set-keep-alive-count", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_keep_alive_count);
        define(linker, store, module, "[method]tcp-socket.hop-limit", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_get_hop_limit);
        define(linker, store, module, "[method]tcp-socket.set-hop-limit", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_hop_limit);
        define(linker, store, module, "[method]tcp-socket.receive-buffer-size", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::tcp_get_receive_buffer_size);
        define(linker, store, module, "[method]tcp-socket.set-receive-buffer-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_receive_buffer_size);
        define(linker, store, module, "[method]tcp-socket.send-buffer-size", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I64)], crate::net::wasi::tcp_get_send_buffer_size);
        define(linker, store, module, "[method]tcp-socket.set-send-buffer-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_set_send_buffer_size);
        define(linker, store, module, "[method]tcp-socket.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::tcp_subscribe);
        define(linker, store, module, "[method]tcp-socket.shutdown", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::tcp_shutdown);
    }
    // wasi:sockets/network@0.2.0
    {
        let module = "wasi:sockets/network@0.2.0";
        define(linker, store, module, "[resource-drop]network", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
    }
    // wasi:sockets/instance-network@0.2.0
    {
        let module = "wasi:sockets/instance-network@0.2.0";
        define(linker, store, module, "instance-network", vec![], vec![ValType::NumType(NumType::I32)], crate::net::wasi::instance_network);
    }
    // wasi:sockets/ip-name-lookup@0.2.0
    {
        let module = "wasi:sockets/ip-name-lookup@0.2.0";
        define(linker, store, module, "[resource-drop]resolve-address-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "resolve-addresses", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::resolve_addresses);
        define(linker, store, module, "[method]resolve-address-stream.resolve-next-address", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::resolve_next_address);
        define(linker, store, module, "[method]resolve-address-stream.subscribe", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::net::wasi::resolve_subscribe);
    }
    // wasi_snapshot_preview1 (Adapter extras)
    {
        let module = "wasi_snapshot_preview1";
        define(linker, store, module, "adapter_close_badfd", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], adapter_close_badfd);
    }
    // wasi:clocks/monotonic-clock@0.2.0
    {
        let module = "wasi:clocks/monotonic-clock@0.2.0";
        define(linker, store, module, "now", vec![], vec![ValType::NumType(NumType::I64)], crate::time::wasi::monotonic_clock_now);
        define(linker, store, module, "resolution", vec![], vec![ValType::NumType(NumType::I64)], crate::time::wasi::monotonic_clock_resolution);
        define(linker, store, module, "subscribe-duration", vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)], crate::time::wasi::monotonic_clock_subscribe_duration);
        define(linker, store, module, "subscribe-instant", vec![ValType::NumType(NumType::I64)], vec![ValType::NumType(NumType::I32)], crate::time::wasi::monotonic_clock_subscribe_instant);
    }
    // wasi:clocks/wall-clock@0.2.0
    {
        let module = "wasi:clocks/wall-clock@0.2.0";
        define(linker, store, module, "now", vec![ValType::NumType(NumType::I32)], vec![], crate::time::wasi::wall_clock_now);
        define(linker, store, module, "resolution", vec![ValType::NumType(NumType::I32)], vec![], crate::time::wasi::wall_clock_resolution);
    }
    // wasi:clocks/timezone@0.2.0
    {
        let module = "wasi:clocks/timezone@0.2.0";
        define(linker, store, module, "display", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], timezone_display);
        define(linker, store, module, "utc-offset", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], timezone_utc_offset);
    }
    // wasi:random/random@0.2.0
    {
        let module = "wasi:random/random@0.2.0";
        define(linker, store, module, "get-random-bytes", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::random::wasi::get_random_bytes);
        define(linker, store, module, "get-random-u64", vec![], vec![ValType::NumType(NumType::I64)], crate::random::wasi::get_random_u64);
    }
    // wasi:random/insecure@0.2.0
    {
        let module = "wasi:random/insecure@0.2.0";
        define(linker, store, module, "get-insecure-random-bytes", vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::random::wasi::get_insecure_random_bytes);
        define(linker, store, module, "get-insecure-random-u64", vec![], vec![ValType::NumType(NumType::I64)], crate::random::wasi::get_insecure_random_u64);
    }
    // wasi:cli/exit@0.2.0
    {
        let module = "wasi:cli/exit@0.2.0";
        define(linker, store, module, "exit", vec![ValType::NumType(NumType::I32)], vec![], crate::process::wasi::exit);
    }
    // wasi:cli/environment@0.2.0
    {
        let module = "wasi:cli/environment@0.2.0";
        define(linker, store, module, "get-environment", vec![ValType::NumType(NumType::I32)], vec![], crate::env::wasi::get_environment);
        define(linker, store, module, "get-arguments", vec![ValType::NumType(NumType::I32)], vec![], crate::env::wasi::get_arguments);
        define(linker, store, module, "initial-cwd", vec![ValType::NumType(NumType::I32)], vec![], crate::env::wasi::initial_cwd);
    }
    // wasi:filesystem/preopens@0.2.0
    {
        let module = "wasi:filesystem/preopens@0.2.0";
        define(linker, store, module, "get-directories", vec![ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::get_directories);
    }
    // wasi:random/insecure-seed@0.2.0
    {
        let module = "wasi:random/insecure-seed@0.2.0";
        define(linker, store, module, "insecure-seed", vec![], vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)], crate::random::wasi::insecure_seed);
    }
    // wasi:cli/run@0.2.0
    {
        let module = "wasi:cli/run@0.2.0";
        define(linker, store, module, "run", vec![], vec![ValType::NumType(NumType::I32)], cli_run);
    }
    // krakeos:graphics/screen@0.2.0
    {
        let module = "krakeos:graphics/screen@0.2.0";
        define(linker, store, module, "get-width", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_screen_width_host);
        define(linker, store, module, "get-height", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_screen_height_host);
    }
    // krakeos:system/process@0.2.0
    {
        let module = "krakeos:system/process@0.2.0";
        define(linker, store, module, "spawn",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            crate::process::wasi::spawn);
        define(linker, store, module, "waitpid",
            vec![ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I32)],
            crate::process::wasi::waitpid);
        define(linker, store, module, "pipe",
            vec![ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::process::wasi::pipe);
        define(linker, store, module, "native-file-open",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I64)],
            crate::process::wasi::native_file_open);
        define(linker, store, module, "native-file-stat",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::process::wasi::native_file_stat);
        define(linker, store, module, "file-read",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I64)],
            crate::process::wasi::file_read);
        define(linker, store, module, "file-write",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I64)],
            crate::process::wasi::file_write);
        define(linker, store, module, "yield",
            vec![],
            vec![],
            crate::process::wasi::yield_host);
        define(linker, store, module, "get-pid",
            vec![],
            vec![ValType::NumType(NumType::I64)],
            crate::os::krakeos::wasi::get_pid_host);
        define(linker, store, module, "get-current-user",
            vec![ValType::NumType(NumType::I32)],
            vec![],
            crate::os::krakeos::wasi::get_current_user_host);
        define(linker, store, module, "get-slot-info",
            vec![ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::get_slot_info_host);
        define(linker, store, module, "set-nonblock",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::set_nonblock_host);
        define(linker, store, module, "ioctl",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::ioctl_host);
        define(linker, store, module, "poll",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::poll_host);
        define(linker, store, module, "kill",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::kill_process_host);
        define(linker, store, module, "syscall",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I64)],
            crate::os::krakeos::wasi::syscall_host);
        define(linker, store, module, "spawn-thread",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I64)],
            crate::os::krakeos::wasi::spawn_thread_host);
        define(linker, store, module, "thread-exit",
            vec![],
            vec![],
            crate::os::krakeos::wasi::thread_exit_host);
    }
    // krakeos:system/memory@0.2.0
    {
        let module = "krakeos:system/memory@0.2.0";
        define(linker, store, module, "shm-get",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            crate::os::krakeos::wasi::shm_get_host_impl);
    }
    // krakeos:system/window@0.2.0
    {
        let module = "krakeos:system/window@0.2.0";
        define(linker, store, module, "create",
            vec![ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            crate::os::krakeos::wasi::window_create_host);
        define(linker, store, module, "update",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)],
            vec![],
            crate::os::krakeos::wasi::window_update_host);
        define(linker, store, module, "get-events",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            crate::os::krakeos::wasi::window_get_events_host);
        define(linker, store, module, "register-event-queue",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![],
            crate::os::krakeos::wasi::register_event_queue_host);
        define(linker, store, module, "deregister-event-queue",
            vec![],
            vec![],
            crate::os::krakeos::wasi::deregister_event_queue_host);
    }
        // wasi:filesystem/types@0.2.0
        {
            let module = "wasi:filesystem/types@0.2.0";
            define(linker, store, module, "[method]descriptor.read-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::read_via_stream);
            define(linker, store, module, "[method]descriptor.write-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::write_via_stream);
            define(linker, store, module, "[method]descriptor.append-via-stream", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::append_via_stream);
            define(linker, store, module, "[method]descriptor.type", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_type);
            define(linker, store, module, "[method]descriptor.stat", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_stat);
            define(linker, store, module, "[method]descriptor.open-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_open_at);
            define(linker, store, module, "[method]descriptor.read-directory", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_read_directory);
            define(linker, store, module, "[method]directory-entry-stream.read-directory-entry", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::directory_entry_stream_read_directory_entry);
            define(linker, store, module, "[method]descriptor.stat-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_stat_at);
            define(linker, store, module, "[method]descriptor.set-times-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_set_times_at);
            define(linker, store, module, "[method]descriptor.link-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_link_at);
            define(linker, store, module, "[method]descriptor.unlink-file-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_unlink_file_at);
            define(linker, store, module, "[method]descriptor.remove-directory-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_remove_directory_at);
            define(linker, store, module, "[method]descriptor.rename-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_rename_at);
            define(linker, store, module, "[method]descriptor.symlink-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_symlink_at);
            define(linker, store, module, "[method]descriptor.readlink-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_readlink_at);
            define(linker, store, module, "[method]descriptor.sync", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_sync);
            define(linker, store, module, "[method]descriptor.set-size", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_set_size);
            define(linker, store, module, "[method]descriptor.set-times", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_set_times);
            define(linker, store, module, "[method]descriptor.seek", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_seek);
            define(linker, store, module, "[method]descriptor.advise", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_advise);
            define(linker, store, module, "[method]descriptor.create-directory-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_create_directory_at);

            define(linker, store, module, "[method]descriptor.get-flags", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_get_flags);
            define(linker, store, module, "[method]descriptor.sync-data", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_sync_data);
            define(linker, store, module, "[method]descriptor.is-same-object", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32)], crate::fs::wasi::descriptor_is_same_object);
            define(linker, store, module, "[method]descriptor.metadata-hash", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_metadata_hash);
            define(linker, store, module, "[method]descriptor.metadata-hash-at", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_metadata_hash_at);
            define(linker, store, module, "[method]descriptor.read", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_read);
            define(linker, store, module, "[method]descriptor.write", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::fs::wasi::descriptor_write);

            define(linker, store, module, "filesystem-error-code", vec![ValType::NumType(NumType::I32)], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::fs::wasi::filesystem_error_code);

        define(linker, store, module, "[resource-drop]descriptor", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);
        define(linker, store, module, "[resource-drop]directory-entry-stream", vec![ValType::NumType(NumType::I32)], vec![], resource_drop);

        // Also add __wasm_call_dtors, __wasi_proc_exit, and __wasi_init_tp to env for compatibility
        let func_type = FuncType { params: ResultType { valtypes: vec![] }, returns: ResultType { valtypes: vec![] } };
        let func_addr = store.func_alloc_unchecked(func_type.clone(), |_, _| Ok(vec![]));
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasm_call_dtors"), ExternVal::Func(func_addr));

        let func_addr = store.func_alloc_unchecked(func_type, |_, _| Ok(vec![]));
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasi_init_tp"), ExternVal::Func(func_addr));

        let exit_type = FuncType { params: ResultType { valtypes: vec![ValType::NumType(NumType::I32)] }, returns: ResultType { valtypes: vec![] } };
        let exit_addr = store.func_alloc_unchecked(exit_type, crate::process::wasi::exit);
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasi_proc_exit"), ExternVal::Func(exit_addr));
    }
}

pub(crate) fn define<T: Config + Clone + Send + 'static>(
    linker: &mut Linker,
    store: &mut Store<'_, T>,
    module: &str,
    name: &str,
    params: Vec<ValType>,
    returns: Vec<ValType>,
    func: for<'a> fn(&mut Store<'a, T>, Vec<Value>) -> Result<Vec<Value>, HaltExecutionError>,
) {
    let func_type = FuncType {
        params: ResultType { valtypes: params },
        returns: ResultType { valtypes: returns },
    };
    let func_addr = store.func_alloc_unchecked(func_type, func);
    let _ = linker.define_unchecked(String::from(module), String::from(name), ExternVal::Func(func_addr));
}

pub(crate) fn resource_drop<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) {
        Some(Value::I32(v)) => *v as i32,
        _ => return Ok(vec![]),
    };
    let wasi = store.wasi_ctx.as_mut().ok_or(HaltExecutionError(1))?;
    wasi.resource_table.remove(&handle);
    Ok(vec![])
}

pub(crate) fn find_cabi_realloc<T: Config>(store: &Store<'_, T>) -> Option<FuncAddr> {
    let module_addr = store.caller_module?;
    if let Ok(export) = store.instance_export(unsafe { Stored::from_bare(module_addr, store.id) }, "cabi_realloc") {
        if let Some(func) = export.as_func() {
            return Some(func.into_bare());
        }
    }
    crate::debugln!("WASI P2 Error: 'cabi_realloc' not found in caller module!");
    None
}

pub(crate) fn call_cabi_realloc<T: Config>(store: &mut Store<'_, T>, new_size: u32, align: u32) -> Result<u32, HaltExecutionError> {
    let cabi_realloc_addr = find_cabi_realloc(store).ok_or(HaltExecutionError(1))?;
    let args = vec![Value::I32(0), Value::I32(0), Value::I32(align), Value::I32(new_size)];
    match store.invoke_unchecked(cabi_realloc_addr, args, None) {
        Ok(RunState::Finished { values, .. }) => {
            if let Some(Value::I32(ptr)) = values.first() {
                Ok(*ptr as u32)
            } else {
                Err(HaltExecutionError(1))
            }
        }
        _ => Err(HaltExecutionError(1)),
    }
}

pub(crate) fn write_bytes<T: Config>(store: &mut Store<'_, T>, addr: u32, bytes: &[u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.init(addr as usize, bytes, 0, bytes.len()).map_err(|_| ())
}

pub(crate) fn write_u32<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u32) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub(crate) fn write_u64<T: Config>(store: &mut Store<'_, T>, addr: u32, val: u64) -> Result<(), ()> {
    write_bytes(store, addr, &val.to_le_bytes())
}

pub(crate) fn read_bytes<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    let module_addr = store.caller_module.ok_or(())?;
    let mem_addr = *store.modules.get(module_addr).mem_addrs.get(0).ok_or(())?;
    let mem = store.memories.get(mem_addr);
    mem.mem.read_slice(addr as usize, buf).map_err(|_| ())
}

pub(crate) fn read_mem<T: Config>(store: &Store<'_, T>, addr: u32, buf: &mut [u8]) -> Result<(), ()> {
    read_bytes(store, addr, buf)
}

pub(crate) fn read_mem_u32<T: Config>(store: &Store<'_, T>, addr: u32) -> Result<u32, HaltExecutionError> {
    let mut buf = [0u8; 4];
    read_mem(store, addr, &mut buf).map_err(|_| HaltExecutionError(1))?;
    Ok(u32::from_le_bytes(buf))
}

pub(crate) fn read_mem_u64<T: Config>(store: &Store<'_, T>, addr: u32) -> Result<u64, HaltExecutionError> {
    let mut buf = [0u8; 8];
    read_mem(store, addr, &mut buf).map_err(|_| HaltExecutionError(1))?;
    Ok(u64::from_le_bytes(buf))
}

pub(crate) fn read_mem_string<T: Config>(store: &Store<'_, T>, ptr: u32) -> Result<String, HaltExecutionError> {
    let mut buf = Vec::new();
    let mut offset = 0;
    loop {
        let mut byte = [0u8; 1];
        read_mem(store, ptr + offset, &mut byte).map_err(|_| HaltExecutionError(1))?;
        if byte[0] == 0 { break; }
        buf.push(byte[0]);
        offset += 1;
        if offset > 4096 { return Err(HaltExecutionError(1)); }
    }
    String::from_utf8(buf).map_err(|_| HaltExecutionError(1))
}

// --- Inline stubs for functions not in scattered wasi.rs files ---

fn cli_run<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)]) // Success
}

fn adapter_close_badfd<T: Config>(_: &mut Store<'_, T>, _args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(8)]) // EBADF
}

fn timezone_display<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let result_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Err(HaltExecutionError(1)) };
    // Write "UTC" string
    let utc = b"UTC";
    let str_ptr = call_cabi_realloc(store, 3, 1)?;
    write_bytes(store, str_ptr, utc).map_err(|_| HaltExecutionError(1))?;
    // Result: (ptr, len, utc_offset, in_dst)
    write_u32(store, result_ptr, str_ptr).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, result_ptr + 4, 3).map_err(|_| HaltExecutionError(1))?;
    write_u32(store, result_ptr + 8, 0).map_err(|_| HaltExecutionError(1))?; // utc_offset = 0
    write_u32(store, result_ptr + 12, 0).map_err(|_| HaltExecutionError(1))?; // in_dst = false
    Ok(vec![])
}

fn timezone_utc_offset<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(0)]) // UTC offset = 0
}
