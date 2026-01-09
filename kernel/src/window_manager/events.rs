use alloc::vec::Vec;
use crate::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]

#[repr(C)]
pub struct MouseEvent {
    pub wid: u32,
    pub x: usize,
    pub y: usize,
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
    pub width: usize,
    pub height: usize,
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
