use core::sync::atomic::{AtomicU32, Ordering};

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

pub const SHARED_EVENT_QUEUE_SIZE: usize = 128;

#[repr(C)]
pub struct SharedEventQueue {
    pub head: AtomicU32,
    pub tail: AtomicU32,
    pub events: [Event; SHARED_EVENT_QUEUE_SIZE],
}

impl SharedEventQueue {
    pub fn push(&self, event: Event) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let next_head = (head + 1) % SHARED_EVENT_QUEUE_SIZE as u32;

        if next_head == self.tail.load(Ordering::Acquire) {
            return false; // Full
        }

        let events_ptr = self.events.as_ptr() as *mut Event;
        unsafe {
            events_ptr.add(head as usize).write(event);
        }

        self.head.store(next_head, Ordering::Release);
        true
    }

    pub fn pop(&self) -> Option<Event> {
        let tail = self.tail.load(Ordering::Relaxed);

        if tail == self.head.load(Ordering::Acquire) {
            return None; // Empty
        }

        let event = self.events[tail as usize];
        self.tail.store((tail + 1) % SHARED_EVENT_QUEUE_SIZE as u32, Ordering::Release);
        Some(event)
    }
}
