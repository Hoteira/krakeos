use alloc::vec::Vec;
use core::slice;
use crate::event::Event;
use crate::widget::{Widget, WidgetId};
use crate::layout::{Display, FlexDirection};
use std::graphics::{self, Items};
use std::os::syscall;
use alloc::string::String;
use std::println;

pub struct FrameBuffer {
    pub address: *mut u32,
    pub size: usize,
}

impl FrameBuffer {
    pub fn new(size: usize) -> Self {
        let address = std::memory::malloc(size) as *mut u32;
        Self { address, size }
    }

    pub fn resize(&mut self, size: usize) {
        if self.size < size {
            let new_addr = std::memory::malloc(size) as *mut u32;
            self.address = new_addr;
            self.size = size;
        }
    }
}

use titanf::TrueTypeFont;

pub struct Window {
    pub id: usize,
    pub title: String,
    pub buffer: FrameBuffer,

    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,

    pub children: Vec<Widget>,

    pub status_bar: bool,
    pub can_move: bool,
    pub can_resize: bool,
    pub min_width: usize,
    pub min_height: usize,

    pub w_type: Items,
    pub focus: WidgetId,

    // Font support
    // We store the data as a static slice (leaked) to satisfy TrueTypeFont lifetime
    pub font: Option<TrueTypeFont>,
}

impl Window {
    pub fn new(title: &str, width: usize, height: usize) -> Self {
        let size = width * height * 4 + 4;

        Window {
            id: 0,
            title: String::from(title),
            buffer: FrameBuffer::new(size),
            x: 0,
            y: 0,
            width,
            height,
            children: Vec::new(),
            status_bar: false,
            can_move: true,
            can_resize: true,
            min_width: 0,
            min_height: 0,
            w_type: Items::Window,
            focus: 0,
            font: None,
        }
    }

    pub fn load_font(&mut self, data: &'static [u8]) {
         if let Ok(font) = TrueTypeFont::load_font(data) {
             self.font = Some(font);
         }
    }

    pub fn show(&mut self) {
        let std_window = graphics::Window {
            id: self.id,
            buffer: self.buffer.address as usize,
            x: self.x,
            y: self.y,
            z: 0,
            width: self.width,
            height: self.height,
            can_move: self.can_move,
            can_resize: self.can_resize,
            min_width: self.min_width,
            min_height: self.min_height,
            event_handler: 1, // Enable event handling
            w_type: self.w_type,
        };

        if self.id == 0 {
            self.id = graphics::add_window(&std_window);
        } else {
            graphics::update_window(&std_window);
        }
        
        self.draw();
        self.update();
    }

    pub fn draw(&mut self) {
        if self.buffer.address.is_null() {
            return;
        }

        let buffer_len = self.buffer.size / 4;
        let buffer = unsafe {
            slice::from_raw_parts_mut(
                self.buffer.address,
                buffer_len
            )
        };
        
        for child in &mut self.children {
            draw_recursive(buffer, self.width, child, 0, 0, self.width, self.height, 0, 0, &mut self.font);
        }
    }

    pub fn update(&mut self) {
        let std_window = graphics::Window {
            id: self.id,
            buffer: self.buffer.address as usize,
            x: self.x,
            y: self.y,
            z: 0,
            width: self.width,
            height: self.height,
            can_move: self.can_move,
            can_resize: self.can_resize,
            min_width: self.min_width,
            min_height: self.min_height,
            event_handler: 1,
            w_type: self.w_type,
        };
        graphics::update_window(&std_window);
    }

    pub fn resize(&mut self, width: usize, height: usize, can_move: bool) {
        if !self.can_resize { return; }
        self.width = width;
        self.height = height;
        self.can_move = can_move;
        
        let new_size = width * height * 4 + 4;
        self.buffer.resize(new_size);
        
        self.draw();
        self.update();
    }

    pub fn event_loop(&mut self) {
        let mut key_buffer = String::with_capacity(64);
        let mut events: [Event; 64] = [Event::None; 64];

        // Syscall 52: SYS_GET_EVENTS
        unsafe {
            syscall(52, self.id as u64, events.as_mut_ptr() as u64, 64);
        }

        for event in events.iter() {
            match event {
                Event::Resize(e) => {
                    if e.width > 0 && e.height > 0 && self.can_resize {
                        self.resize(e.width, e.height, self.can_move);
                    }
                },
                Event::Mouse(e) => {
                    let btn = self.find_interactive_widget_at(e.x, e.y);
                    if let Some(btn) = btn {
                        if let Some(handler) = btn.get_event_handler() {
                            let btn_id = btn.get_id();
                            handler(self, btn_id);
                        } else {
                            self.focus = btn.get_id();
                        }
                    }
                },
                Event::Keyboard(e) => {
                    // Simple keyboard handling
                    if e.char == '\x08' { // Backspace
                        if !key_buffer.is_empty() {
                            key_buffer.pop();
                        }
                    } else {
                        key_buffer.push(e.char);
                    }
                },
                _ => {},
            }
        }
    }

    pub fn find_interactive_widget_at(&self, x: usize, y: usize) -> Option<&Widget> {
        for child in &self.children {
            if let Some(widget) = find_interactive_widget_recursive(child, x, y) {
                return Some(widget);
            }
        }
        None
    }

    pub fn find_widget_by_id_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        for child in self.children.iter_mut() {
            if let Some(found) = child.find_widget_by_id_mut(id) {
                return Some(found);
            }
        }
        None
    }

    pub fn find_widget_by_id(&self, id: WidgetId) -> Option<&Widget> {
        for child in &self.children {
            if let Some(found) = child.find_widget_by_id(id) {
                return Some(found);
            }
        }
        None
    }
}

fn find_interactive_widget_recursive(widget: &Widget, x: usize, y: usize) -> Option<&Widget> {
    let geometry = widget.geometry();

    if x < geometry.x || x >= geometry.x + geometry.width ||
       y < geometry.y || y >= geometry.y + geometry.height {
        return None;
    }

    if let Some(children) = widget.get_children() {
        for child in children.iter().rev() {
            if let Some(found) = find_interactive_widget_recursive(child, x, y) {
                return Some(found);
            }
        }
        None
    } else {
        Some(widget)
    }
}


pub fn draw_recursive(
    buffer: &mut [u32],
    width0: usize,
    widget: &mut Widget,
    parent_x: usize,
    parent_y: usize,
    parent_width: usize,
    parent_height: usize,
    parent_padding: usize,
    _parent_margin: usize,
    font: &mut Option<TrueTypeFont>,
) {
    // Update layout first
    widget.update_layout(parent_x, parent_y, parent_width, parent_height, parent_padding, _parent_margin, &Display::None);

    // Draw the current widget
    widget.draw(buffer, width0, font);

    let widget_x = widget.get_x();
    let widget_y = widget.get_y();
    let widget_width = widget.get_width();
    let widget_height = widget.get_height();
    let widget_padding = widget.get_padding();
    let widget_margin = widget.get_margin();

    let display = widget.get_display();

    if let Some(children) = widget.get_children_mut() {
        match display {
            Display::Flex { direction, wrap } => {
                let content_x = widget_x + widget_padding;
                let content_y = widget_y + widget_padding;
                let content_width = widget_width.saturating_sub(widget_padding * 2);
                let content_height = widget_height.saturating_sub(widget_padding * 2);

                let mut child_info = Vec::new();
                for child in children.iter_mut() {
                    child.update_layout(content_x, content_y, content_width, content_height, 0, widget_margin, &Display::None);
                    let child_geom = child.geometry();
                    child_info.push((child_geom.width + child_geom.margin * 2, child_geom.height + child_geom.margin * 2));
                }

                let mut current_x = 0;
                let mut current_y = 0;
                let mut line_height = 0;
                let mut line_width = 0;

                for (i, child) in children.iter_mut().enumerate() {
                    let (child_total_w, child_total_h) = child_info[i];
                    let (child_x, child_y, child_w, child_h) = match direction {
                        FlexDirection::Row => {
                            if wrap && current_x + child_total_w > content_width && current_x > 0 {
                                current_x = 0;
                                current_y += line_height;
                                line_height = 0;
                            }
                            let x = content_x + current_x;
                            let y = content_y + current_y;
                            let w = child_total_w.min(content_width - current_x);
                            let h = child_total_h.min(content_height - current_y);
                            current_x += child_total_w;
                            line_height = line_height.max(child_total_h);
                            (x, y, w, h)
                        },
                        FlexDirection::Column => {
                            if wrap && current_y + child_total_h > content_height && current_y > 0 {
                                current_y = 0;
                                current_x += line_width;
                                line_width = 0;
                            }
                            let x = content_x + current_x;
                            let y = content_y + current_y;
                            let w = child_total_w.min(content_width - current_x);
                            let h = child_total_h.min(content_height - current_y);
                            current_y += child_total_h;
                            line_width = line_width.max(child_total_w);
                            (x, y, w, h)
                        }
                    };
                    draw_recursive(buffer, width0, child, child_x, child_y, child_w, child_h, 0, widget_margin, font);
                }
            },
            Display::Grid { rows, cols } => {
                if rows > 0 && cols > 0 {
                    let content_width = widget_width.saturating_sub(widget_padding * 2);
                    let content_height = widget_height.saturating_sub(widget_padding * 2);
                    let cell_width = content_width / cols;
                    let cell_height = content_height / rows;

                    for (i, child) in children.iter_mut().enumerate() {
                        let row = i / cols;
                        let col = i % cols;
                        if row >= rows { break; }
                        let child_x = widget_x + widget_padding + col * cell_width;
                        let child_y = widget_y + widget_padding + row * cell_height;
                        draw_recursive(buffer, width0, child, child_x, child_y, cell_width, cell_height, 0, widget_margin, font);
                    }
                }
            },
            Display::None => {
                // FIX: We need to actually draw the children!
                let content_x = widget_x + widget_padding;
                let content_y = widget_y + widget_padding;
                let content_width = widget_width.saturating_sub(widget_padding * 2);
                let content_height = widget_height.saturating_sub(widget_padding * 2);

                // Take children temporarily to avoid borrow issues
                let children_vec = core::mem::take(children);

                for mut child in children_vec.into_iter() {
                    // Recursively draw each child
                    draw_recursive(buffer, width0, &mut child, content_x, content_y, content_width, content_height, 0, widget_margin, font);
                    children.push(child);
                }
            }
        }
    }
}