use crate::sync::Mutex;
use crate::rust_alloc::string::String;
use crate::rust_alloc::vec::Vec;

static ARGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static VARS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

pub fn args() -> Args {
    let guard = ARGS.lock();
    Args {
        iter: guard.clone().into_iter(),
    }
}

pub struct Args {
    iter: crate::rust_alloc::vec::IntoIter<String>,
}

impl Iterator for Args {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

pub fn vars() -> Vars {
    let guard = VARS.lock();
    Vars {
        iter: guard.clone().into_iter(),
    }
}

pub struct Vars {
    iter: crate::rust_alloc::vec::IntoIter<(String, String)>,
}

impl Iterator for Vars {
    type Item = (String, String);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

pub fn var(key: &str) -> Result<String, VarError> {
    let guard = VARS.lock();
    for (k, v) in guard.iter() {
        if k == key {
            return Ok(v.clone());
        }
    }
    Err(VarError::NotPresent)
}

#[derive(Debug, PartialEq, Eq)]
pub enum VarError {
    NotPresent,
    NotUnicode(String), 
}

// Internal initialization function called by runtime
pub(crate) fn init_args(raw_args: &[String]) {
    let mut guard = ARGS.lock();
    *guard = raw_args.to_vec();
}

pub(crate) fn init_vars(raw_vars: &[(String, String)]) {
    let mut guard = VARS.lock();
    *guard = raw_vars.to_vec();
}