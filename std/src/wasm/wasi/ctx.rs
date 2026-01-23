use super::env::WasiEnv;
use crate::rust_alloc::boxed::Box;
use crate::rust_alloc::collections::BTreeMap;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;


pub struct WasiCtx {
    pub resource_table: BTreeMap<i32, WasiResource>,
    pub next_resource_id: i32,
    pub env: Box<dyn WasiEnv>,
}

impl WasiCtx {
    pub fn new(args: Vec<String>, root_path: String, fds: &[(u8, u8)]) -> Self {
        let mut resource_table = BTreeMap::new();
        resource_table.insert(3, WasiResource::Directory(String::from("/")));

        Self {
            resource_table,
            next_resource_id: 4,
            env: Box::new(super::krakeos::KrakeosWasiEnv::new(args, root_path, fds)),
        }
    }
}

impl Default for WasiCtx {
    fn default() -> Self {
        Self::new(Vec::new(), String::from("@0xE0"), &[(0, 0), (1, 1), (2, 2)])
    }
}

#[derive(Debug)]
pub enum WasiResource {
    InputStream(InputStreamSource),
    OutputStream(OutputStreamSource),
    Pollable(PollableTarget),
    File(crate::fs::File),
    Directory(String),
}

#[derive(Clone, Copy, Debug)]
pub enum PollableTarget {
    Timer(u64),
    Read(i32),
    Write(i32),
}

#[derive(Clone, Debug)]
pub enum InputStreamSource {
    Null,
    Stdin,
    File(usize),
}

#[derive(Clone, Debug)]
pub enum OutputStreamSource {
    Null,
    Stdout,
    Stderr,
    File(usize),
}