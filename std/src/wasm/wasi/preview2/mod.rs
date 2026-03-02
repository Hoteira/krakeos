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

pub fn create_wasi_p2_imports<T: Config>(linker: &mut Linker, store: &mut Store<'_, T>) {
    if store.wasi_ctx.is_none() {
        store.wasi_ctx = Some(crate::wasm::wasi::ctx::WasiCtx::default());
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
        define(linker, store, module, "get-terminal-stdin", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stdin);
    }
    // wasi:cli/terminal-stdout@0.2.0
    {
        let module = "wasi:cli/terminal-stdout@0.2.0";
        define(linker, store, module, "get-terminal-stdout", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stdout);
    }
    // wasi:cli/terminal-stderr@0.2.0
    {
        let module = "wasi:cli/terminal-stderr@0.2.0";
        define(linker, store, module, "get-terminal-stderr", vec![], vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_terminal_stderr);
    }

    // wasi:cli/stdout@0.2.0
    {
        let module = "wasi:cli/stdout@0.2.0";
        define(linker, store, module, "get-stdout", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stdout);
    }
    // wasi:cli/stdin@0.2.0
    {
        let module = "wasi:cli/stdin@0.2.0";
        define(linker, store, module, "get-stdin", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stdin);
    }
    // wasi:cli/stderr@0.2.0
    {
        let module = "wasi:cli/stderr@0.2.0";
        define(linker, store, module, "get-stderr", vec![], vec![ValType::NumType(NumType::I32)], crate::os::krakeos::wasi::get_stderr);
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
        define(linker, store, module, "[method]outgoing-datagram-stream.send", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_send);
        define(linker, store, module, "[method]incoming-datagram-stream.receive", vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)], vec![], crate::net::wasi::udp_receive);
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
        define(linker, store, module, "get-random-u64", vec![], vec![ValType::NumType(NumType::I64)], crate::random::wasi::get_insecure_random_u64); // Reused
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
        define(linker, store, module, "get-environment", vec![], vec![ValType::NumType(NumType::I32)], crate::env::wasi::get_environment);
        define(linker, store, module, "get-arguments", vec![], vec![ValType::NumType(NumType::I32)], crate::env::wasi::get_arguments);
        define(linker, store, module, "initial-cwd", vec![], vec![ValType::NumType(NumType::I32)], crate::env::wasi::get_environment);
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
        define(linker, store, module, "get-width", vec![], vec![ValType::NumType(NumType::I32)], get_screen_width_host);
        define(linker, store, module, "get-height", vec![], vec![ValType::NumType(NumType::I32)], get_screen_height_host);
    }
    // krakeos:system/process@0.2.0
    {
        let module = "krakeos:system/process@0.2.0";
        define(linker, store, module, "spawn",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            process_spawn_host);
        define(linker, store, module, "waitpid",
            vec![ValType::NumType(NumType::I64)],
            vec![ValType::NumType(NumType::I32)],
            process_waitpid_host);
        define(linker, store, module, "pipe",
            vec![ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            process_pipe_host);
        define(linker, store, module, "yield",
            vec![],
            vec![],
            crate::process::wasi::yield_host);
    }
    // krakeos:system/memory@0.2.0
    {
        let module = "krakeos:system/memory@0.2.0";
        define(linker, store, module, "shm-get",
            vec![ValType::NumType(NumType::I32), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            shm_get_host);
    }
    // krakeos:system/window@0.2.0
    {
        let module = "krakeos:system/window@0.2.0";
        define(linker, store, module, "create",
            vec![ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I64)],
            window_create_host);
        define(linker, store, module, "update",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32)],
            vec![],
            window_update_host);
        define(linker, store, module, "get-events",
            vec![ValType::NumType(NumType::I64), ValType::NumType(NumType::I32), ValType::NumType(NumType::I32)],
            vec![ValType::NumType(NumType::I32)],
            window_get_events_host);
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

        // Also add __wasm_call_dtors and __wasi_proc_exit to env for compatibility
        let func_type = FuncType { params: ResultType { valtypes: vec![] }, returns: ResultType { valtypes: vec![] } };
        let func_addr = store.func_alloc_unchecked(func_type, |_, _| Ok(vec![]));
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasm_call_dtors"), ExternVal::Func(func_addr));

        let exit_type = FuncType { params: ResultType { valtypes: vec![ValType::NumType(NumType::I32)] }, returns: ResultType { valtypes: vec![] } };
        let exit_addr = store.func_alloc_unchecked(exit_type, crate::process::wasi::exit);
        let _ = linker.define_unchecked(String::from("env"), String::from("__wasi_proc_exit"), ExternVal::Func(exit_addr));
    }
}

pub(crate) fn define<T: Config>(
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

// --- Host implementations for krakeos-specific WASI ---

fn get_screen_width_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let w = crate::os::graphics::get_screen_width();
    Ok(vec![Value::I32(w as u32)])
}

fn get_screen_height_host<T: Config>(_: &mut Store<'_, T>, _: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    Ok(vec![Value::I32(crate::os::graphics::get_screen_height() as u32)])
}

fn process_spawn_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let get_arg = |i: usize| -> u32 {
        match args.get(i) {
            Some(Value::I32(v)) => *v as u32,
            _ => 0
        }
    };

    let path_ptr = get_arg(0);
    let path_len = get_arg(1);
    let argv_ptr = get_arg(2);
    let argv_len = get_arg(3);
    let fds_ptr = get_arg(4);
    let fds_len = get_arg(5);

    let mut path_buf = vec![0u8; path_len as usize];
    read_mem(store, path_ptr, &mut path_buf).map_err(|_| HaltExecutionError(1))?;
    let path = String::from_utf8_lossy(&path_buf);

    let mut host_args = Vec::new();
    for i in 0..argv_len {
        let arg_ptr_ptr = argv_ptr + i * 4;
        let arg_ptr = read_mem_u32(store, arg_ptr_ptr)? as u32;
        let arg = read_mem_string(store, arg_ptr)?;
        host_args.push(arg);
    }
    let host_args_refs: Vec<&str> = host_args.iter().map(|s| s.as_str()).collect();

    let mut host_fds = Vec::new();
    for i in 0..fds_len {
        let fd_ptr = fds_ptr + i * 2;
        let mut buf = [0u8; 2];
        read_mem(store, fd_ptr, &mut buf).map_err(|_| HaltExecutionError(1))?;
        host_fds.push((buf[0], buf[1]));
    }

    let pid = crate::os::spawn_with_fds(&path, &host_args_refs, &host_fds);
    Ok(vec![Value::I64(pid as u64)])
}

fn process_waitpid_host<T: Config>(_store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let pid = match args.get(0) { Some(Value::I64(v)) => *v as u64, _ => 0 };
    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe { crate::sys::syscall(61, pid, 0, 0) as i32 };
    #[cfg(target_arch = "wasm32")]
    let res = unsafe { crate::os::krakeos::process_waitpid(pid) };

    Ok(vec![Value::I32(res as u32)])
}

fn process_pipe_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32((-1i32) as u32)]) };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut fds = [0i32; 2];
        let res = unsafe { crate::sys::syscall(22, fds.as_mut_ptr() as u64, 0, 0) as i32 };
        if res == 0 {
            let mut bytes = [0u8; 8];
            bytes[0..4].copy_from_slice(&fds[0].to_le_bytes());
            bytes[4..8].copy_from_slice(&fds[1].to_le_bytes());
            write_bytes(store, ptr, &bytes).map_err(|_| HaltExecutionError(1))?;
            Ok(vec![Value::I32(0)])
        } else {
            Ok(vec![Value::I32((-1i32) as u32)])
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut bytes = [0u8; 8];
        let res = unsafe { crate::os::krakeos::process_pipe(bytes.as_mut_ptr()) };
        if res == 0 {
            write_bytes(store, ptr, &bytes).map_err(|_| HaltExecutionError(1))?;
            Ok(vec![Value::I32(0)])
        } else {
            Ok(vec![Value::I32((-1i32) as u32)])
        }
    }
}

fn window_create_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };

    let id = read_mem_u32(store, ptr)? as usize;
    let buffer_off = read_mem_u32(store, ptr + 4)? as u64;
    let back_buffer_off = read_mem_u32(store, ptr + 8)? as u64;
    let flipped_off = read_mem_u32(store, ptr + 12)? as u64;
    let pid = read_mem_u64(store, ptr + 16)?;
    let x = read_mem_u32(store, ptr + 24)? as i32 as isize;
    let y = read_mem_u32(store, ptr + 28)? as i32 as isize;
    let z = read_mem_u32(store, ptr + 32)? as usize;
    let width = read_mem_u32(store, ptr + 36)? as usize;
    let height = read_mem_u32(store, ptr + 40)? as usize;

    let mut bools = [0u8; 4];
    read_mem(store, ptr + 44, &mut bools).map_err(|_| HaltExecutionError(1))?;

    let min_width = read_mem_u32(store, ptr + 48)? as usize;
    let min_height = read_mem_u32(store, ptr + 52)? as usize;
    let event_handler = read_mem_u32(store, ptr + 56)? as usize;
    let w_type_val = read_mem_u32(store, ptr + 60)?;
    let prev_x = read_mem_u32(store, ptr + 64)? as i32 as isize;
    let prev_y = read_mem_u32(store, ptr + 68)? as i32 as isize;
    let prev_width = read_mem_u32(store, ptr + 72)? as usize;
    let prev_height = read_mem_u32(store, ptr + 76)? as usize;

    let wasm_base = store.get_wasm_base_ptr() as u64;

    let host_win = crate::os::graphics::Window {
        id,
        buffer: if buffer_off != 0 { (wasm_base + buffer_off) as usize } else { 0 },
        back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) as usize } else { 0 },
        flipped: if flipped_off != 0 { (wasm_base + flipped_off) as usize } else { 0 },
        pid,
        x,
        y,
        z,
        width,
        height,
        can_move: bools[0] != 0,
        can_resize: bools[1] != 0,
        transparent: bools[2] != 0,
        treat_as_transparent: bools[3] != 0,
        min_width,
        min_height,
        event_handler,
        w_type: unsafe { core::mem::transmute(w_type_val) },
        prev_x,
        prev_y,
        prev_width,
        prev_height,
    };

    #[cfg(not(target_arch = "wasm32"))]
    let res = unsafe { crate::sys::syscall(100, &host_win as *const _ as u64, 0, 0) };
    #[cfg(target_arch = "wasm32")]
    let res = 0;

    if res != 0 {
        let _ = write_bytes(store, ptr, &(res as u32).to_le_bytes());
    }

    Ok(vec![Value::I64(res)])
}

fn window_update_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let _handle = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
    let ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![]) };

    let id = read_mem_u32(store, ptr)? as usize;
    let buffer_off = read_mem_u32(store, ptr + 4)? as u64;
    let back_buffer_off = read_mem_u32(store, ptr + 8)? as u64;
    let flipped_off = read_mem_u32(store, ptr + 12)? as u64;
    let pid = read_mem_u64(store, ptr + 16)?;
    let x = read_mem_u32(store, ptr + 24)? as i32 as isize;
    let y = read_mem_u32(store, ptr + 28)? as i32 as isize;
    let z = read_mem_u32(store, ptr + 32)? as usize;
    let width = read_mem_u32(store, ptr + 36)? as usize;
    let height = read_mem_u32(store, ptr + 40)? as usize;

    let mut bools = [0u8; 4];
    read_mem(store, ptr + 44, &mut bools).map_err(|_| HaltExecutionError(1))?;

    let min_width = read_mem_u32(store, ptr + 48)? as usize;
    let min_height = read_mem_u32(store, ptr + 52)? as usize;
    let event_handler = read_mem_u32(store, ptr + 56)? as usize;
    let w_type_val = read_mem_u32(store, ptr + 60)?;
    let prev_x = read_mem_u32(store, ptr + 64)? as i32 as isize;
    let prev_y = read_mem_u32(store, ptr + 68)? as i32 as isize;
    let prev_width = read_mem_u32(store, ptr + 72)? as usize;
    let prev_height = read_mem_u32(store, ptr + 76)? as usize;

    let wasm_base = store.get_wasm_base_ptr() as u64;

    let host_win = crate::os::graphics::Window {
        id,
        buffer: if buffer_off != 0 { (wasm_base + buffer_off) as usize } else { 0 },
        back_buffer: if back_buffer_off != 0 { (wasm_base + back_buffer_off) as usize } else { 0 },
        flipped: if flipped_off != 0 { (wasm_base + flipped_off) as usize } else { 0 },
        pid,
        x,
        y,
        z,
        width,
        height,
        can_move: bools[0] != 0,
        can_resize: bools[1] != 0,
        transparent: bools[2] != 0,
        treat_as_transparent: bools[3] != 0,
        min_width,
        min_height,
        event_handler,
        w_type: unsafe { core::mem::transmute(w_type_val) },
        prev_x,
        prev_y,
        prev_width,
        prev_height,
    };

    #[cfg(not(target_arch = "wasm32"))]
    unsafe { crate::sys::syscall(102, &host_win as *const _ as u64, 0, 0); }

    Ok(vec![])
}

fn window_get_events_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let handle = match args.get(0) { Some(Value::I64(v)) => *v, _ => 0 };
    let buf_ptr = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(0)]) };
    let max = match args.get(2) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I32(0)]) };

    let event_size = core::mem::size_of::<crate::os::graphics::Event>();
    let mut buf = vec![0u8; max as usize * event_size];

    #[cfg(not(target_arch = "wasm32"))]
    let count = unsafe { crate::sys::syscall(104, handle, buf.as_mut_ptr() as u64, max as u64) as i32 };
    #[cfg(target_arch = "wasm32")]
    let count = 0;

    if count > 0 {
        write_bytes(store, buf_ptr, &buf[..count as usize * event_size]).map_err(|_| HaltExecutionError(1))?;
    }

    Ok(vec![Value::I32(count as u32)])
}

fn shm_get_host<T: Config>(store: &mut Store<'_, T>, args: Vec<Value>) -> Result<Vec<Value>, HaltExecutionError> {
    let name_ptr = match args.get(0) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
    let name_len = match args.get(1) { Some(Value::I32(v)) => *v as u32, _ => return Ok(vec![Value::I64(0)]) };
    let size = match args.get(2) { Some(Value::I32(v)) => *v as usize, _ => return Ok(vec![Value::I64(0)]) };

    let mut name_buf = vec![0u8; name_len as usize];
    read_mem(store, name_ptr, &mut name_buf).map_err(|_| HaltExecutionError(1))?;
    let name = String::from_utf8_lossy(&name_buf);

    #[cfg(not(target_arch = "wasm32"))]
    unsafe {
        let mut name_terminated = String::from(name);
        name_terminated.push('\0');
        let res = crate::sys::syscall(120, name_terminated.as_ptr() as u64, name_terminated.len() as u64, size as u64);
        Ok(vec![Value::I64(res)])
    }
    #[cfg(target_arch = "wasm32")]
    Ok(vec![Value::I64(0)])
}
