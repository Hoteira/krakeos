use crate::interrupts::task::{ThreadState, TASK_MANAGER};
use crate::sync::Mutex;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum AsyncEvent {
    Generic(u64),
    IO(i32),
    Timer(u64),
    Read(i32),
    Write(i32),
}

pub struct EventRegistration {
    pub thread_idx: usize,
    pub event: AsyncEvent,
}

pub struct PendingEvent {
    pub thread_idx: usize,
    pub event: AsyncEvent,
}

pub struct EventManager {
    pub registrations: Vec<EventRegistration>,
    pub pending: Vec<PendingEvent>,
}

pub static EVENT_MANAGER: Mutex<EventManager> = Mutex::new(EventManager {
    registrations: Vec::new(),
    pending: Vec::new(),
});

impl EventManager {
    pub fn register(&mut self, thread_idx: usize, event: AsyncEvent) {
        if thread_idx >= crate::interrupts::task::MAX_THREADS { return; }

        for reg in &self.registrations {
            if reg.thread_idx == thread_idx && reg.event == event {
                return;
            }
        }
        self.registrations.push(EventRegistration { thread_idx, event });
    }

    pub fn check_pending(&mut self, thread_idx: usize, event: AsyncEvent) -> bool {
        let mut found = false;
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].thread_idx == thread_idx && self.pending[i].event == event {
                self.pending.remove(i);
                found = true;
                break;
            } else {
                i += 1;
            }
        }
        found
    }

    pub fn check_timers(&mut self, tm: &mut crate::interrupts::task::TaskManager, current_ticks: u64) {
        let mut i = 0;
        while i < self.registrations.len() {
            if let AsyncEvent::Timer(target) = self.registrations[i].event {
                if current_ticks >= target {
                    let reg = self.registrations.remove(i);
                    if let Some(thread) = &mut tm.tasks[reg.thread_idx] {
                        if thread.state == ThreadState::WaitingForEvent {
                            thread.state = ThreadState::Ready;
                        }
                    }
                    // When a thread wakes, clear its other registrations
                    self.registrations.retain(|r| r.thread_idx != reg.thread_idx);
                    // Since we modified the vec, we should probably restart or be careful.
                    // Actually, retain() is better but we are in a loop.
                    // Let's just restart the loop for simplicity after a retain.
                    i = 0;
                    continue;
                }
            }
            i += 1;
        }
    }

    pub fn signal_tids(&mut self, tids: &[u64]) {
        for &tid in tids {

            // Internal call, assumes locks are handled or not needed

            // Actually, better to use the public signal

        }
    }


    pub fn signal_with_latch(&mut self, tm: &mut crate::interrupts::task::TaskManager, event: AsyncEvent) {
        let mut woken_tids = Vec::new();

        let mut i = 0;

        while i < self.registrations.len() {
            if self.registrations[i].event == event {
                let reg = self.registrations.remove(i);

                if let Some(thread) = &mut tm.tasks[reg.thread_idx] {
                    if thread.state == ThreadState::WaitingForEvent {
                        thread.state = ThreadState::Ready;

                        woken_tids.push(reg.thread_idx);
                    }
                }
            } else {
                i += 1;
            }
        }


        let woken = !woken_tids.is_empty();


        // Clean up other registrations for ALL woken threads

        for tid in woken_tids {
            self.registrations.retain(|r| r.thread_idx != tid);
        }


        if !woken { // Logic to store signals as pending if no one was waiting


            // Store as pending for this specific thread if possible

            let thread_idx = match event {
                AsyncEvent::Generic(tid) => Some(tid as usize),

                _ => None, // For I/O and timers, we don't have a specific thread target usually

            };


            if let Some(target_idx) = thread_idx {
                if target_idx < crate::interrupts::task::MAX_THREADS {
                    let mut exists = false;


                    for p in &self.pending {
                        if p.thread_idx == target_idx && p.event == event {
                            exists = true;


                            break;
                        }
                    }


                    if !exists {
                        self.pending.push(PendingEvent { thread_idx: target_idx, event });
                    }
                }
            } else {


                // For non-targeted events, we might need a different latching strategy


                // but usually I/O readiness is persistent at the source.

            }
        }
    }


    pub fn unregister_thread(&mut self, thread_idx: usize) {
        self.registrations.retain(|reg| reg.thread_idx != thread_idx);

        self.pending.retain(|p| p.thread_idx != thread_idx);
    }
}


pub fn signal_event(event: AsyncEvent) {
    let mut tm = TASK_MANAGER.int_lock();

    let mut em = EVENT_MANAGER.int_lock();

    em.signal_with_latch(&mut tm, event);
}

    