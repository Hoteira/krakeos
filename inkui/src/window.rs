use crate::font::Font;
use crate::event::{scancode_to_char, Event, KeyboardEvent};
use crate::layout::Display;
use crate::types::Color;
use crate::widget::{Widget, WidgetId};
use std::fs::File;
use std::io::{Read, Write};

/// A top-level window backed by the KrakeOS window device.
///
/// Presenting = writing `[x:u32][y:u32][w:u32][h:u32]` + BGRA pixels to
/// `/dev/gpu/window`; the kernel compositor picks it up on its next tick.
/// A header with `x == 0xFFFF_FFFF` is a resize request (w,h = new size).
pub struct Window {
    file: File,
    keyboard: Option<File>,

    pub title: String,
    pub width: usize,
    pub height: usize,

    pub children: Vec<Widget>,
    pub focus: WidgetId,
    pub font: Option<Font>,
    pub background: Color,

    buffer: Vec<u32>,
    dirty: bool,
}

impl Window {
    /// Opens a window. Size is clamped to the screen (1024x552 usable).
    pub fn new(title: &str, width: usize, height: usize) -> Option<Self> {
        let file = File::options().read(true).write(true).open("/dev/gpu/window").ok()?;
        let keyboard = File::options().read(true).open("/dev/input/keyboard").ok();

        let width = width.clamp(32, 1024);
        let height = height.clamp(32, 552);

        let mut win = Window {
            file,
            keyboard,
            title: String::from(title),
            width,
            height,
            children: Vec::new(),
            focus: 0,
            font: Font::load_default(),
            background: Color::rgb(255, 255, 255),
            buffer: vec![0xFF000000u32; width * height],
            dirty: true,
        };
        win.request_size(width, height);
        Some(win)
    }

    /// Ask the kernel to resize this window's buffer.
    fn request_size(&mut self, w: usize, h: usize) {
        let mut header = Vec::with_capacity(16);
        header.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());
        header.extend_from_slice(&(w as u32).to_le_bytes());
        header.extend_from_slice(&(h as u32).to_le_bytes());
        let _ = self.file.write_all(&header);
    }

    pub fn load_font(&mut self, path: &str) {
        self.font = Font::load(path);
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn add_child(&mut self, widget: Widget) {
        self.children.push(widget);
        self.dirty = true;
    }

    /// Layout + paint the widget tree and present it.
    pub fn draw(&mut self) {
        let n = self.width * self.height;
        if self.buffer.len() != n {
            self.buffer.clear();
            self.buffer.resize(n, 0xFF000000);
        }
        self.buffer.fill(self.background.to_u32());

        for child in &mut self.children {
            child.update_layout(0, 0, self.width, self.height, 0, 0, &Display::None);
        }
        for child in &mut self.children {
            paint_recursive(&mut self.buffer, self.width, child, &mut self.font);
        }

        self.present();
        self.dirty = false;
    }

    /// Send the buffer to the kernel compositor.
    fn present(&mut self) {
        let mut data = Vec::with_capacity(16 + self.buffer.len() * 4);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(self.width as u32).to_le_bytes());
        data.extend_from_slice(&(self.height as u32).to_le_bytes());
        let pixels: &[u8] = unsafe {
            std::slice::from_raw_parts(self.buffer.as_ptr() as *const u8, self.buffer.len() * 4)
        };
        data.extend_from_slice(pixels);
        let _ = self.file.write_all(&data);
    }

    pub fn show(&mut self) {
        self.mark_dirty();
        self.update();
    }

    pub fn update(&mut self) {
        if self.dirty {
            self.draw();
        }
    }

    /// Drain pending keyboard events (kernel queue, 8 bytes per event).
    pub fn poll_events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        let Some(kb) = self.keyboard.as_mut() else { return out };
        let mut buf = [0u8; 8];
        while let Ok(8) = kb.read(&mut buf) {
            let type_ = u16::from_le_bytes([buf[0], buf[1]]);
            let code = u16::from_le_bytes([buf[2], buf[3]]);
            let value = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
            if type_ == 1 {
                let key = scancode_to_char(code).map(|c| c as u32).unwrap_or(0);
                out.push(Event::Keyboard(KeyboardEvent {
                    key,
                    code,
                    pressed: value == 1 || value == 2,
                    repeat: 1,
                }));
            }
        }
        out
    }

    pub fn focus_next(&mut self) {
        let mut ids = Vec::new();
        for child in &self.children {
            collect_focusable_widgets(child, &mut ids);
        }
        if ids.is_empty() {
            return;
        }

        let current_idx = ids.iter().position(|&id| id == self.focus);
        let next_id = match current_idx {
            Some(idx) => ids[(idx + 1) % ids.len()],
            None => ids[0],
        };

        if self.focus != next_id {
            if self.focus != 0 {
                if let Some(w) = self.find_widget_by_id_mut(self.focus) {
                    w.set_focused(false);
                }
            }
            self.focus = next_id;
            if let Some(w) = self.find_widget_by_id_mut(self.focus) {
                w.set_focused(true);
            }
            self.mark_dirty();
            self.update();
        }
    }

    /// One iteration of the standard event loop: keyboard focus traversal
    /// (Tab), activation (Enter/Space on buttons), and text input.
    pub fn event_loop(&mut self) {
        let events = self.poll_events();
        let mut any_redraw = false;

        for event in events.iter() {
            if let Event::Keyboard(e) = event {
                if !e.pressed {
                    continue;
                }
                let char_opt = if e.key != 0 { core::char::from_u32(e.key) } else { None };

                if e.key == 9 {
                    self.focus_next();
                    continue;
                }

                if self.focus != 0 {
                    let mut click_handler: Option<fn(&mut Window, WidgetId)> = None;

                    if let Some(widget) = self.find_widget_by_id_mut(self.focus) {
                        match widget {
                            Widget::Button { on_click, .. } => {
                                if e.key == 10 || e.key == 13 || e.key == 32 {
                                    click_handler = *on_click;
                                }
                            }
                            Widget::TextInput { on_submit, .. } => {
                                if e.key == 10 || e.key == 13 {
                                    click_handler = *on_submit;
                                } else if let Some(c) = char_opt {
                                    widget.handle_key(c);
                                    any_redraw = true;
                                }
                            }
                            _ => {
                                if let Some(c) = char_opt {
                                    widget.handle_key(c);
                                    any_redraw = true;
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
        }

        if any_redraw {
            self.mark_dirty();
            self.update();
        }
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

pub fn paint_recursive(
    buffer: &mut [u32],
    width0: usize,
    widget: &mut Widget,
    font: &mut Option<Font>,
) {
    widget.draw(buffer, width0, font);

    if let Some(children) = widget.get_children_mut() {
        for child in children {
            paint_recursive(buffer, width0, child, font);
        }
    }
}
