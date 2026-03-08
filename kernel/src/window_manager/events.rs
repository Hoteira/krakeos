use crate::sync::Mutex;
use alloc::vec::Vec;
pub use std::os::krakeos::events::{Event, EventQueueHeader, KeyboardEvent, MouseEvent, ResizeEvent};

pub const QUEUE_SIZE: usize = 256;

pub struct EventQueue {
    pub queue: [Event; QUEUE_SIZE],
    pub head: usize,
    pub tail: usize,
    pub count: usize,
}

pub static GLOBAL_EVENT_QUEUE: Mutex<EventQueue> = Mutex::new(EventQueue {
    queue: [Event::None; QUEUE_SIZE],
    head: 0,
    tail: 0,
    count: 0,
});

impl EventQueue {
    pub fn init(&mut self) {}

    pub fn add_event(&mut self, mut event: Event) {
        if let Event::Keyboard(ref mut kb) = event {
            if kb.repeat == 0 { kb.repeat = 1; }
        }

        if self.count >= QUEUE_SIZE {
            self.tail = (self.tail + 1) % QUEUE_SIZE;
            self.count -= 1;
        }

        self.queue[self.head] = event;
        self.head = (self.head + 1) % QUEUE_SIZE;
        self.count += 1;
    }

    pub fn push_to_process(&self, tm: &crate::interrupts::task::TaskManager, pid: u64, event: Event) -> bool {
        use core::sync::atomic::Ordering;

        let thread = tm.tasks.iter().flatten().find(|t| {
            t.state != crate::interrupts::task::ThreadState::Zombie
                && t.process.as_ref().map_or(false, |p| p.pid == pid)
        });
        if let Some(thread) = thread {
            let proc = thread.process.as_ref().unwrap();
            let (header_ptr, buf_ptr, capacity) = *proc.event_queue.lock();
            if header_ptr == 0 {
                return false;
            }
            let header = unsafe { &*(header_ptr as *const EventQueueHeader) };
            let head = header.head.load(Ordering::Relaxed);
            let next_head = (head + 1) % capacity;
            if next_head == header.tail.load(Ordering::Acquire) {
                return true; // queue full — swallow silently (process is registered)
            }
            unsafe { (buf_ptr as *mut Event).add(head as usize).write(event); }
            header.head.store(next_head, Ordering::Release);
            return true;
        }
        false
    }

    pub fn get_and_remove_events(&mut self, window_id: u32, max_events: usize) -> Vec<Event> {
        let mut result = Vec::with_capacity(max_events);


        let mut processed_count = 0;
        let initial_count = self.count;
        let mut current_idx = self.tail;


        if self.count == 0 { return result; }


        let mut new_count = 0;
        let mut read_ptr = self.tail;
        let mut write_ptr = self.tail;

        for _ in 0..self.count {
            let evt = self.queue[read_ptr];

            let mut taken = false;
            if evt.get_window_id() == window_id && result.len() < max_events {
                result.push(evt);
                taken = true;
            }

            if !taken {
                self.queue[write_ptr] = evt;
                write_ptr = (write_ptr + 1) % QUEUE_SIZE;
                new_count += 1;
            }

            read_ptr = (read_ptr + 1) % QUEUE_SIZE;
        }

        self.head = write_ptr;
        self.count = new_count;

        result
    }

    pub fn reset_queue(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.count = 0;
    }
}
