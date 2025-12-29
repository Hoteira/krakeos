use alloc::vec::Vec;
use core::slice;
use crate::event::Event;
use crate::widget::{Widget, WidgetId};
use crate::layout::Display;
use std::graphics::{self, Items};
use std::os::syscall;
use alloc::string::String;

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
    pub pid: u64,

    pub x: isize,
    pub y: isize,
    pub width: usize,
    pub height: usize,

    pub children: Vec<Widget>,

    pub status_bar: bool,
    pub can_move: bool,
    pub can_resize: bool,
    pub transparent: bool,
    pub treat_as_transparent: bool,
    pub min_width: usize,
    pub min_height: usize,

    pub w_type: Items,
    pub focus: WidgetId,

    pub font: Option<TrueTypeFont>,
}

impl Window {
    pub fn new(title: &str, width: usize, height: usize) -> Self {
        let size = width * height * 4 + 4;

        Window {
            id: 0,
            title: String::from(title),
            buffer: FrameBuffer::new(size),
            pid: 0,
            x: 0,
            y: 0,
            width,
            height,
            children: Vec::new(),
            status_bar: false,
            can_move: true,
            can_resize: true,
            transparent: true,
            treat_as_transparent: true,
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

    pub fn set_transparent(&mut self, transparent: bool) {
        self.transparent = transparent;
    }

    pub fn set_treat_as_transparent(&mut self, treat: bool) {
        self.treat_as_transparent = treat;
    }

    pub fn show(&mut self) {
        let std_window = graphics::Window {
            id: self.id,
            buffer: self.buffer.address as usize,
            pid: self.pid,
            x: self.x,
            y: self.y,
            z: 0,
            width: self.width,
            height: self.height,
            can_move: self.can_move,
            can_resize: self.can_resize,
            transparent: self.transparent,
            treat_as_transparent: self.treat_as_transparent,
            min_width: self.min_width,
            min_height: self.min_height,
            event_handler: 1, 
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
        
        buffer.fill(0);
        
        for child in &mut self.children {
            child.update_layout(0, 0, self.width, self.height, 0, 0, &Display::None);
        }

        for child in &mut self.children {
            paint_recursive(buffer, self.width, child, &mut self.font);
        }
    }

    pub fn draw_widget(&mut self, _id: WidgetId) {
        self.draw();
    }

    pub fn update(&mut self) {
        let std_window = graphics::Window {
            id: self.id,
            buffer: self.buffer.address as usize,
            pid: self.pid,
            x: self.x,
            y: self.y,
            z: 0,
            width: self.width,
            height: self.height,
            can_move: self.can_move,
            can_resize: self.can_resize,
            transparent: self.transparent,
            treat_as_transparent: self.treat_as_transparent,
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

    pub fn focus_next(&mut self) {
        let mut ids = Vec::new();
        for child in &self.children {
            collect_focusable_widgets(child, &mut ids);
        }
        
        if ids.is_empty() { return; }
        
        let current_idx = ids.iter().position(|&id| id == self.focus);
        
        let next_id = match current_idx {
            Some(idx) => ids[(idx + 1) % ids.len()],
            None => ids[0],
        };

        let old_focus = self.focus;
        if old_focus != next_id {
            if old_focus != 0 {
                if let Some(w) = self.find_widget_by_id_mut(old_focus) {
                    w.set_focused(false);
                }
                self.draw_widget(old_focus);
            }
            
            self.focus = next_id;
            if let Some(w) = self.find_widget_by_id_mut(self.focus) {
                w.set_focused(true);
            }
            self.draw_widget(self.focus);
            self.update();
        }
    }

    pub fn event_loop(&mut self) {
        let mut key_buffer = String::with_capacity(64);
        let mut events: [Event; 64] = [Event::None; 64];

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
                    let target_id = if let Some(widget) = self.find_interactive_widget_at(e.x, e.y) {
                        Some(widget.get_id())
                    } else {
                        None
                    };

                    if e.scroll != 0 {
                        let scroll_target = if self.focus != 0 {
                             Some(self.focus)
                        } else {
                             target_id
                        };

                        if let Some(id) = scroll_target {
                            if let Some(w) = self.find_widget_by_id_mut(id) {
                                w.handle_scroll(e.scroll);
                                self.draw_widget(id);
                                self.update();
                            }
                        }
                    }

                    if let Some(new_id) = target_id {
                        if self.focus != new_id {
                            if self.focus != 0 {
                                if let Some(old_w) = self.find_widget_by_id_mut(self.focus) {
                                    old_w.set_focused(false);
                                }
                            }
                            
                            self.focus = new_id;
                            if let Some(new_w) = self.find_widget_by_id_mut(self.focus) {
                                new_w.set_focused(true);
                                self.draw_widget(self.focus);
                                self.update();
                            }
                        }
                    }

                    if e.buttons[0] { 
                        if let Some(id) = target_id {
                            let mut handler_opt = None;
                            if let Some(w) = self.find_widget_by_id(id) {
                                handler_opt = w.get_event_handler();
                            }

                            if let Some(handler) = handler_opt {
                                handler(self, id);
                            }
                        }
                    }
                },
                Event::Keyboard(e) => {
                    let char_opt = if e.key < 0x110000 {
                        core::char::from_u32(e.key)
                    } else {
                        None
                    };

                    if e.key == 9 { 
                        self.focus_next();
                        return;
                    }

                    if self.focus != 0 {
                        let mut needs_redraw = false;
                        let mut click_handler: Option<fn(&mut Window, WidgetId)> = None;

                        if let Some(widget) = self.find_widget_by_id_mut(self.focus) {
                            match widget {
                                Widget::Button { on_click, .. } => {
                                    if e.pressed && (e.key == 13 || e.key == 32) { 
                                        click_handler = *on_click;
                                    }
                                },
                                Widget::TextInput { on_submit, .. } => {
                                    if e.pressed && (e.key == 13 || e.key == 10) { 
                                        click_handler = *on_submit;
                                    } else if e.pressed {
                                        if let Some(c) = char_opt {
                                            for _ in 0..e.repeat {
                                                widget.handle_key(c);
                                            }
                                            needs_redraw = true;
                                        }
                                    }
                                },
                                _ => {
                                    if e.pressed {
                                        if let Some(c) = char_opt {
                                            for _ in 0..e.repeat {
                                                widget.handle_key(c);
                                            }
                                            needs_redraw = true;
                                        }
                                    }
                                }
                            }
                        }
                        
                        if needs_redraw {
                            self.draw_widget(self.focus);
                            self.update();
                        }

                        if let Some(handler) = click_handler {
                            handler(self, self.focus);
                        }
                    }
                    
                    if let Some(c) = char_opt {
                        for _ in 0..e.repeat {
                            if c == '\x08' {
                                if !key_buffer.is_empty() { key_buffer.pop(); }
                            } else {
                                key_buffer.push(c);
                            }
                        }
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

fn collect_focusable_widgets(widget: &Widget, ids: &mut Vec<WidgetId>) {
    match widget {
        Widget::Button { .. } | Widget::TextInput { .. } => {
            ids.push(widget.get_id());
        },
        _ => {}
    }
    
    if let Some(children) = widget.get_children() {
        for child in children {
            collect_focusable_widgets(child, ids);
        }
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


pub fn paint_recursive(
    buffer: &mut [u32],
    width0: usize,
    widget: &mut Widget,
    font: &mut Option<TrueTypeFont>,
) {
    widget.draw(buffer, width0, font);

    if let Some(children) = widget.get_children_mut() {
        for child in children {
            paint_recursive(buffer, width0, child, font);
        }
    }
}