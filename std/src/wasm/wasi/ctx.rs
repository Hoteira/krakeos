use crate::rust_alloc::collections::BTreeMap;
use crate::rust_alloc::string::String;
use crate::rust_alloc::boxed::Box;
use super::env::WasiEnv;

pub struct WasiCtx {
    pub resource_table: BTreeMap<i32, WasiResource>,
    pub next_resource_id: i32,
    pub env: Box<dyn WasiEnv>,
}

impl Default for WasiCtx {
    fn default() -> Self {
        let mut resource_table = BTreeMap::new();
        // Preopen the root directory at ID 3
        resource_table.insert(3, WasiResource::Directory(String::from("/")));

        Self {
            resource_table,
            next_resource_id: 4,
            env: Box::new(super::krakeos::KrakeosWasiEnv::default()),
        }
    }
}

pub enum WasiResource {
    InputStream(InputStreamSource),
    OutputStream(OutputStreamSource),
    Pollable(PollableTarget),
    File(crate::fs::File),
    Directory(String),
}

#[derive(Clone, Copy)]
pub enum PollableTarget {
    Timer(u64),
    Read(i32),
    Write(i32),
}

#[derive(Clone)]
pub enum InputStreamSource {
    Null,
    Stdin,
    File(usize),
}

#[derive(Clone)]
pub enum OutputStreamSource {
    Null,
    Stdout,
    Stderr,
    File(usize),
}