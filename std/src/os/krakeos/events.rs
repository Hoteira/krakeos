use core::sync::atomic::{AtomicU32, Ordering};

extern crate alloc;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct MouseEvent {
    pub wid: u32,
    pub x: u32,
    pub y: u32,
    pub buttons: [bool; 3],
    pub scroll: i8,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct KeyboardEvent {
    pub wid: u32,
    pub key: u32,
    pub pressed: bool,
    pub repeat: u16,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct ResizeEvent {
    pub wid: u32,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub struct RedrawEvent {
    pub wid: u32,
    pub to_fb: bool,
    pub to_db: bool,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(C)]
pub enum Event {
    Mouse(MouseEvent),
    Keyboard(KeyboardEvent),
    Resize(ResizeEvent),
    Redraw(RedrawEvent),
    None,
}

impl Event {
    pub fn get_window_id(&self) -> u32 {
        match self {
            Event::Mouse(event) => event.wid,
            Event::Keyboard(event) => event.wid,
            Event::Resize(event) => event.wid,
            Event::Redraw(event) => event.wid,
            Event::None => 0,
        }
    }
}

/// Control block for the event queue. Lives in the WASM heap alongside the
/// backing buffer. The kernel holds a pointer to this and uses head/tail to
/// push events directly into the buffer without any kernel-side allocation.
#[repr(C)]
pub struct EventQueueHeader {
    pub head: AtomicU32,
    pub tail: AtomicU32,
    pub capacity: u32,
    pub _pad: u32,
}

/// A userland-owned event queue backed by a `Vec<Event>` in the WASM heap.
/// Allocate one, call `register()` so the kernel knows where to push events,
/// then drive your event loop with `pop()`. `deregister()` is called
/// automatically on drop.
pub struct EventQueue {
    header: alloc::boxed::Box<EventQueueHeader>,
    buf: alloc::boxed::Box<[Event]>,
}

impl EventQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            header: alloc::boxed::Box::new(EventQueueHeader {
                head: AtomicU32::new(0),
                tail: AtomicU32::new(0),
                capacity: capacity as u32,
                _pad: 0,
            }),
            buf: alloc::vec![Event::None; capacity].into_boxed_slice(),
        }
    }

    /// Tell the kernel where this queue lives so it can push events here directly.
    pub fn register(&self) {
        let header_ptr = &*self.header as *const EventQueueHeader as u64;
        let buf_ptr = self.buf.as_ptr() as u64;
        super::register_event_queue(header_ptr, buf_ptr, self.header.capacity as u64);
    }

    /// Tell the kernel to stop writing to this queue. Called automatically on drop.
    pub fn deregister(&self) {
        super::deregister_event_queue();
    }

    /// Pop the next event. Returns `None` if the queue is empty.
    pub fn pop(&self) -> Option<Event> {
        let tail = self.header.tail.load(Ordering::Relaxed);
        if tail == self.header.head.load(Ordering::Acquire) {
            return None;
        }
        
        let tail_idx = (tail % self.header.capacity) as usize;
        
        // Safety: kernel wrote this slot with Release before advancing head;
        // we observed head != tail so the slot is initialised.
        let event = unsafe { self.buf.as_ptr().add(tail_idx).read() };
        self.header
            .tail
            .store((tail + 1) % self.header.capacity, Ordering::Release);
        Some(event)
    }
}

impl Drop for EventQueue {
    fn drop(&mut self) {
        self.deregister();
    }
}
