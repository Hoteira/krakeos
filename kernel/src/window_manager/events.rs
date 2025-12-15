use alloc::vec::Vec;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
    pub char: char,
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
    None
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

pub static mut GLOBAL_EVENT_QUEUE: EventQueue = EventQueue { queue: Vec::new() };

pub struct EventQueue {
    pub queue: Vec<Event>,
}

impl EventQueue {
    pub fn get_and_remove_events(&mut self, window_id: u32, max_events: usize) -> Vec<Event> {
        let mut result = Vec::with_capacity(max_events.min(self.queue.len()));
        let mut write_idx = 0;
        let mut read_idx = 0;

        while read_idx < self.queue.len() && result.len() < max_events {
            if self.queue[read_idx].get_window_id() == window_id {
                result.push(self.queue[read_idx]);
                read_idx += 1;
            } else {
                if write_idx != read_idx {
                    self.queue[write_idx] = self.queue[read_idx];
                }
                write_idx += 1;
                read_idx += 1;
            }
        }

        while read_idx < self.queue.len() {
            if write_idx != read_idx {
                self.queue[write_idx] = self.queue[read_idx];
            }
            write_idx += 1;
            read_idx += 1;
        }

        self.queue.truncate(write_idx);
        result
    }

    pub fn add_event(&mut self, event: Event) {
        if self.queue.len() >= 1000 {
            self.reset_queue();
        }
        self.queue.push(event);
    }

    pub fn reset_queue(&mut self) {
        self.queue.clear();
    }
}
