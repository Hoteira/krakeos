use core::ffi::{c_int, c_void};
use std::thread::JoinHandle;
use std::sync::Mutex;
use alloc::collections::BTreeMap;

pub type pthread_t = usize;

static HANDLES: Mutex<BTreeMap<pthread_t, JoinHandle<usize>>> = Mutex::new(BTreeMap::new());

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_create(
    thread: *mut pthread_t,
    _attr: *const c_void,
    start_routine: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    arg: *mut c_void,
) -> c_int {
    let arg_addr = arg as usize;
    let func_addr = start_routine as usize;

    let handle = std::thread::spawn(move || {
        let f: unsafe extern "C" fn(*mut c_void) -> *mut c_void = core::mem::transmute(func_addr);
        let res = unsafe { f(arg_addr as *mut c_void) };
        res as usize
    });

    let tid = handle.thread_id();
    if !thread.is_null() {
        *thread = tid;
    }

    let mut handles = HANDLES.lock();
    handles.insert(tid, handle);

    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_join(thread: pthread_t, retval: *mut *mut c_void) -> c_int {
    let handle = {
        let mut handles = HANDLES.lock();
        handles.remove(&thread)
    };

    if let Some(h) = handle {
        if let Ok(res) = h.join() {
            if !retval.is_null() {
                *retval = res as *mut c_void;
            }
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_exit(retval: *mut c_void) -> ! {
    // This is tricky because we need to pass the retval back to the JoinHandle's packet
    // But since we are inside the thread, we don't easily have access to it here 
    // unless we use thread local storage (which we haven't implemented).
    
    // For now, we'll just exit. retval support would require TLS or similar.
    let _ = retval;
    crate::stdlib::exit(0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_self() -> pthread_t {
    crate::unistd::getpid() as pthread_t
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn pthread_detach(thread: pthread_t) -> c_int {
    let handle = {
        let mut handles = HANDLES.lock();
        handles.remove(&thread)
    };

    if let Some(h) = handle {
        h.detach();
        0
    } else {
        -1
    }
}
