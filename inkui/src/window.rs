use crate::event::Event;
use crate::layout::Display;
use crate::widget::{Widget, WidgetId};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::slice;
use core::sync::atomic::{AtomicBool, Ordering};
use std::os::graphics::{self, Items};
use std::sync::Mutex;

static SIDE_QUEUE: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

pub struct FrameBuffer {
    pub front: Vec<u32>,
    pub back: Vec<u32>,
    pub size: usize,
    pub flipped: AtomicBool,
}

impl FrameBuffer {
    pub fn new(size: usize) -> Self {
        let u32_len = (size + 3) / 4;
        std::os::debug_print("[inkui] FrameBuffer::new: allocating...\n");
        let front = alloc::vec![0u32; u32_len];
        std::os::debug_print("[inkui] FrameBuffer::new: front done\n");
        let back = alloc::vec![0u32; u32_len];
        std::os::debug_print("[inkui] FrameBuffer::new: back done\n");
        Self {
            front,
            back,
            size,
            flipped: AtomicBool::new(false),
        }
    }

    pub fn resize(&mut self, size: usize) {
        if self.size < size {
            let u32_len = (size + 3) / 4;
            self.front = alloc::vec![0u32; u32_len];
            self.back = alloc::vec![0u32; u32_len];
            self.size = size;
        }
    }

    pub fn flip(&self) {
        let current = self.flipped.load(Ordering::Relaxed);
        self.flipped.store(!current, Ordering::Release);
    }

    pub fn active_address(&self) -> *mut u32 {
        if self.flipped.load(Ordering::Acquire) {
            self.back.as_ptr() as *mut u32
        } else {
            self.front.as_ptr() as *mut u32
        }
    }

    pub fn inactive_address(&self) -> *mut u32 {
        if self.flipped.load(Ordering::Acquire) {
            self.front.as_ptr() as *mut u32
        } else {
            self.back.as_ptr() as *mut u32
        }
    }
}

use titanf::TrueTypeFont;
use std::println;

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
    pub event_queue: std::os::EventQueue,

    dirty: bool,
}

impl Window {
    pub fn new(title: &str, width: usize, height: usize) -> Self {
        let size = width * height * 4 + 4;

        let pid = std::process::get_pid();
        std::debugln!("[inkui] Creating window '{}' with pid {}", title, pid);
        let event_queue = std::os::EventQueue::new(512);
        event_queue.register();

        Window {
            id: 0,
            title: String::from(title),
            buffer: FrameBuffer::new(size),
            pid,
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
            event_queue,
            dirty: true,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
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
            id: self.id as u64,
            buffer: self.buffer.front.as_ptr() as u64,
            back_buffer: self.buffer.back.as_ptr() as u64,
            flipped: &self.buffer.flipped as *const _ as u64,
            pid: self.pid,
            x: self.x as i64,
            y: self.y as i64,
            z: 0,
            width: self.width as u64,
            height: self.height as u64,
            can_move: self.can_move,
            can_resize: self.can_resize,
            transparent: self.transparent,
            treat_as_transparent: self.treat_as_transparent,
            _pad0: [0; 4],
            min_width: self.min_width as u64,
            min_height: self.min_height as u64,
            event_handler: 1,
            w_type: self.w_type,
            _pad1: [0; 4],
            prev_x: 0,
            prev_y: 0,
            prev_width: 0,
            prev_height: 0,
        };

        if self.id == 0 {
            self.id = graphics::add_window(&std_window);
        } else {
            graphics::update_window(&std_window);
        }

        self.mark_dirty();
        self.update();
    }

    pub fn draw(&mut self) {
        let new_size = self.width * self.height * 4 + 4;
        self.buffer.resize(new_size);

        let addr = self.buffer.inactive_address();
        if addr.is_null() {
            return;
        }

        let buffer_len = self.buffer.size / 4;
        let buffer = unsafe {
            slice::from_raw_parts_mut(
                addr,
                buffer_len,
            )
        };

        buffer.fill(0);

        for child in &mut self.children {
            child.update_layout(0, 0, self.width, self.height, 0, 0, &Display::None);
        }

        for child in &mut self.children {
            paint_recursive(buffer, self.width, child, &mut self.font);
        }

        self.buffer.flip();
        self.dirty = false;
    }

    pub fn draw_widget(&mut self, _id: WidgetId) {
        self.draw();
    }

    /// Notify the compositor of current window state (dimensions, buffer pointers)
    /// without redrawing content. Must be called after buffer reallocation so the
    /// compositor doesn't hold dangling pointers.
    fn sync_window(&self) {
        let std_window = graphics::Window {
            id: self.id as u64,
            buffer: self.buffer.front.as_ptr() as u64,
            back_buffer: self.buffer.back.as_ptr() as u64,
            flipped: &self.buffer.flipped as *const _ as u64,
            pid: self.pid,
            x: self.x as i64,
            y: self.y as i64,
            z: 0,
            width: self.width as u64,
            height: self.height as u64,
            can_move: self.can_move,
            can_resize: self.can_resize,
            transparent: self.transparent,
            treat_as_transparent: self.treat_as_transparent,
            _pad0: [0; 4],
            min_width: self.min_width as u64,
            min_height: self.min_height as u64,
            event_handler: 1,
            w_type: self.w_type,
            _pad1: [0; 4],
            prev_x: 0,
            prev_y: 0,
            prev_width: 0,
            prev_height: 0,
        };
        graphics::update_window(&std_window);
    }

    pub fn update(&mut self) {
        if self.dirty {
            self.draw();
        }
        self.sync_window();
    }

    pub fn update_area(&mut self, x: usize, y: usize, w: usize, h: usize) {
        graphics::update_window_area(self.id, x, y, w, h);
    }

    pub fn resize(&mut self, width: usize, height: usize, x: isize, y: isize, can_move: bool) {
        if !self.can_resize { return; }
        std::debugln!("[inkui] Resizing window '{}' to {}x{} at ({}, {})", self.title, width, height, x, y);
        self.width = width;
        self.height = height;
        self.x = x;
        self.y = y;
        self.can_move = can_move;

        self.mark_dirty();
    }

    pub fn poll_events(&mut self) -> Vec<Event> {
        let mut vec = Vec::new();

        // 1. Check side queue first
        {
            let mut side = SIDE_QUEUE.lock();
            let mut i = 0;
            while i < side.len() {
                if side[i].get_window_id() == self.id as u32 || side[i].get_window_id() == 0 {
                    vec.push(side.remove(i).unwrap());
                    if vec.len() >= 64 { return vec; }
                } else {
                    i += 1;
                }
            }
        }

        // 2. Drain registered queue
        while let Some(e) = self.event_queue.pop() {
            if e.get_window_id() == self.id as u32 || e.get_window_id() == 0 {
                vec.push(e);
            } else {
                SIDE_QUEUE.lock().push_back(e);
            }
            if vec.len() >= 64 { break; }
        }

        // 3. Fallback to legacy syscall if still empty
        if vec.is_empty() {
            let mut events: [Event; 64] = [Event::None; 64];
            graphics::get_events(self.id, &mut events);

            for e in events {
                if e == Event::None {
                    break;
                }
                vec.push(e);
            }
        }

        // 4. Coalesce resize events: only keep the last one
        let resize_count = vec.iter().filter(|e| matches!(e, Event::Resize(_))).count();
        if resize_count > 1 {
            let last_resize = vec.iter().rev().find(|e| matches!(e, Event::Resize(_))).copied();
            vec.retain(|e| !matches!(e, Event::Resize(_)));
            if let Some(r) = last_resize {
                vec.push(r);
            }
        }

        vec
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
        let events = self.poll_events();
        let mut any_redraw = false;

        for event in events.iter() {
            match event {
                Event::Resize(_) => {} // handled below after all other events
                Event::Mouse(e) => {
                    let target_id = if let Some(widget) = self.find_interactive_widget_at(e.x as usize, e.y as usize) {
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
                                any_redraw = true;
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
                                any_redraw = true;
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
                                any_redraw = true;
                            }
                        }
                    }
                }
                Event::Keyboard(e) => {
                    let char_opt = if e.key < 0x110000 {
                        core::char::from_u32(e.key)
                    } else {
                        None
                    };

                    if e.key == 9 {
                        self.focus_next();
                        continue;
                    }

                    if self.focus != 0 {
                        let mut click_handler: Option<fn(&mut Window, WidgetId)> = None;

                        if let Some(widget) = self.find_widget_by_id_mut(self.focus) {
                            match widget {
                                Widget::Button { on_click, .. } => {
                                    if e.pressed && (e.key == 13 || e.key == 32) {
                                        click_handler = *on_click;
                                    }
                                }
                                Widget::TextInput { on_submit, .. } => {
                                    if e.pressed && (e.key == 13 || e.key == 10) {
                                        click_handler = *on_submit;
                                    } else if e.pressed {
                                        if let Some(c) = char_opt {
                                            for _ in 0..e.repeat {
                                                widget.handle_key(c);
                                            }
                                            any_redraw = true;
                                        }
                                    }
                                }
                                _ => {
                                    if e.pressed {
                                        if let Some(c) = char_opt {
                                            for _ in 0..e.repeat {
                                                widget.handle_key(c);
                                            }
                                            any_redraw = true;
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(handler) = click_handler {
                            handler(self, self.focus);
                            any_redraw = true;
                        }
                    }
                }
                _ => {}
            }
        }

        // Apply only the last resize event (poll_events already coalesced)
        if let Some(Event::Resize(e)) = events.iter().rev().find(|e| matches!(e, Event::Resize(_))) {
            if e.width > 0 && e.height > 0 && self.can_resize {
                self.resize(e.width as usize, e.height as usize, e.x as isize, e.y as isize, self.can_move);
                any_redraw = true;
            }
        }

        if any_redraw {
            self.mark_dirty();
            self.update();
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
        }
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