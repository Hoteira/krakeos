use alloc::string::String;
use alloc::vec::Vec;
use crate::layout::{Display, FlexDirection};
use crate::types::{Align, Color, Size};
use crate::math::ceil_f32;
use crate::window::Window;

pub type WidgetId = usize;
pub type EventHandler = fn(&mut Window, WidgetId);

#[derive(Debug, Clone)]
pub struct Text {
    pub text: String,
    pub size: usize,
    pub color: Color,
    pub align: Align,
    pub font: String,
    pub max_len: Option<usize>,
    pub can_modify: bool,
    pub min_len: usize,
}

impl Text {
    pub  fn new(text: &str) -> Self {
        Text {
            text: String::from(text),
            size: 12,
            color: Color::rgb(0, 0, 0),
            align: Align::Left,
            font: String::from("default"),
            max_len: None,
            can_modify: false,
            min_len: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WidgetGeometry {
    pub id: WidgetId,

    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub margin: usize,
    pub padding: usize,

    pub user_x: Size,
    pub user_y: Size,
    pub user_width: Size,
    pub user_height: Size,
    pub user_margin: Size,
    pub user_padding: Size,

    pub border_color: Color,
    pub border_radius: Size,
    pub border_size: Size,
}

impl WidgetGeometry {
    pub fn new(id: WidgetId) -> Self {
        WidgetGeometry {
            id,

            x: 0,
            y: 0,
            width: 0,
            height: 0,
            margin: 0,
            padding: 0,

            user_x: Size::Absolute(0),
            user_y: Size::Absolute(0),
            user_width: Size::Absolute(100),
            user_height: Size::Absolute(30),
            user_margin: Size::Absolute(0),
            user_padding: Size::Absolute(0),
            border_color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            border_radius: Size::Auto,
            border_size: Size::Auto,
        }
    }
}

use crate::types::BackgroundStyle;

#[derive(Debug, Clone)]
pub enum Widget {
    Frame {
        geometry: WidgetGeometry,
        children: Vec<Widget>,
        display: Display,
        background: BackgroundStyle,
    },

    Button {
        geometry: WidgetGeometry,
        text: Text,
        background: BackgroundStyle,
        on_click: Option<EventHandler>,
    },

    Label {
        geometry: WidgetGeometry,
        text: Text,
        background: BackgroundStyle,
        writable: bool,
    },

    Canvas {
        geometry: WidgetGeometry,
        framebuffer: Vec<u32>,
        background: BackgroundStyle,
    },
}
impl Widget {
    pub fn frame(id: WidgetId) -> Self {
        Widget::Frame {
            geometry: WidgetGeometry::new(id),
            children: Vec::new(),
            display: Display::None,
            background: BackgroundStyle::solid(Color::rgb(255, 255, 255)),
        }
    }

    pub fn button(id: WidgetId, text: &str) -> Self {
        Widget::Button {
            geometry: WidgetGeometry::new(id),
            text: Text::new(text),
            background: BackgroundStyle::solid(Color::rgb(200, 200, 200)),
            on_click: None,
        }
    }

    pub fn label(id: WidgetId, text: &str) -> Self {
        Widget::Label {
            geometry: WidgetGeometry::new(id),
            text: Text::new(text),
            background: BackgroundStyle::solid(Color::rgb(255, 255, 255)),
            writable: false,
        }
    }

    pub fn canvas(id: WidgetId) -> Self {
        Widget::Canvas {
            geometry: WidgetGeometry::new(id),
            framebuffer: Vec::new(),
            background: BackgroundStyle::solid(Color::rgb(100, 100, 100)),
        }
    }

    // UPDATE background_color method
    pub fn background_color(mut self, color: Color) -> Self {
        match &mut self {
            Widget::Frame { background, .. } |
            Widget::Button { background, .. } |
            Widget::Label { background, .. } |
            Widget::Canvas { background, .. } => {
                *background = BackgroundStyle::solid(color);    
            }
        }
        self
    }

    pub fn background_gradient(mut self, gradient: LinearGradient) -> Self {
        match &mut self {
            Widget::Frame { background, .. } |
            Widget::Button { background, .. } |
            Widget::Label { background, .. } |
            Widget::Canvas { background, .. } => {
                *background = BackgroundStyle::gradient(gradient);
            }
        }
        self
    }

    // Fluent API
    pub fn width(mut self, width: Size) -> Self {
        self.geometry_mut().user_width = width;
        self
    }

    pub fn height(mut self, height: Size) -> Self {
        self.geometry_mut().user_height = height;
        self
    }

    pub fn padding(mut self, padding: Size) -> Self {
        self.geometry_mut().user_padding = padding;
        self
    }

    pub fn margin(mut self, margin: Size) -> Self {
        self.geometry_mut().user_margin = margin;
        self
    }

    pub fn x(mut self, x: Size) -> Self {
        self.geometry_mut().user_x = x;
        self
    }

    pub fn y(mut self, y: Size) -> Self {
        self.geometry_mut().user_y = y;
        self
    }

    pub fn set_border_radius(mut self, radius: Size) -> Self {
        self.geometry_mut().border_radius = radius;
        self
    }

    pub fn on_click(mut self, handler: fn(&mut Window, WidgetId)) -> Self {
        if let Widget::Button { on_click, .. } = &mut self {
            *on_click = Some(handler);
        }
        self
    }

    pub fn add_child(mut self, child: Widget) -> Self {
        if let Widget::Frame { children, .. } = &mut self {
            children.push(child);
        }
        self
    }

    pub fn set_text_color(mut self, color: Color) -> Self {
        match &mut self {
            Widget::Button { text, .. } |
            Widget::Label { text, .. } => {
                text.color = color;
            }
            _ => {}
        }
        self
    }

    pub fn set_text_align(mut self, align: Align) -> Self {
        match &mut self {
            Widget::Button { text, .. } |
            Widget::Label { text, .. } => {
                text.align = align;
            }
            _ => {}
        }
        self
    }

    pub fn set_text_size(mut self, size: usize) -> Self {
        match &mut self {
            Widget::Button { text, .. } |
            Widget::Label { text, .. } => {
                text.size = size;
            }
            _ => {}
        }
        self
    }

    // Geometry getters
    pub fn geometry(&self) -> &WidgetGeometry {
        match self {
            Widget::Frame { geometry, .. } |
            Widget::Button { geometry, .. } |
            Widget::Label { geometry, .. } |
            Widget::Canvas { geometry, .. } => geometry,
        }
    }

    pub fn geometry_mut(&mut self) -> &mut WidgetGeometry {
        match self {
            Widget::Frame { geometry, .. } |
            Widget::Button { geometry, .. } |
            Widget::Label { geometry, .. } |
            Widget::Canvas { geometry, .. } => geometry,
        }
    }

    pub fn get_id(&self) -> WidgetId {
        self.geometry().id
    }

    pub fn get_x(&self) -> usize {
        self.geometry().x
    }

    pub fn get_y(&self) -> usize {
        self.geometry().y
    }

    pub fn get_width(&self) -> usize {
        self.geometry().width
    }

    pub fn get_height(&self) -> usize {
        self.geometry().height
    }

    pub fn get_margin(&self) -> usize {
        self.geometry().margin
    }

    pub fn get_padding(&self) -> usize {
        self.geometry().padding
    }

    pub fn get_user_width(&self) -> Size {
        self.geometry().user_width
    }

    pub fn get_user_height(&self) -> Size {
        self.geometry().user_height
    }

    pub fn get_user_margin(&self) -> Size {
        self.geometry().user_margin
    }

    pub fn get_user_padding(&self) -> Size {
        self.geometry().user_padding
    }

    pub fn set_x(&mut self, val: usize) {
        self.geometry_mut().x = val;
    }

    pub fn set_y(&mut self, val: usize) {
        self.geometry_mut().y = val;
    }

    pub fn set_width(&mut self, val: usize) {
        self.geometry_mut().width = val;
    }

    pub fn set_height(&mut self, val: usize) {
        self.geometry_mut().height = val;
    }

    pub fn get_children(&self) -> Option<&Vec<Widget>> {
        match self {
            Widget::Frame { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn get_children_mut(&mut self) -> Option<&mut Vec<Widget>> {
        match self {
            Widget::Frame { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn get_event_handler(&self) -> Option<EventHandler> {
        match self {
            Widget::Button { on_click, .. } => *on_click,
            _ => None,
        }
    }

    pub fn get_display(&self) -> Display {
        match self {
            Widget::Frame { display, .. } => *display,
            _ => Display::None,
        }
    }

    pub fn update_layout(
        &mut self,
        parent_x: usize,
        parent_y: usize,
        parent_width: usize,
        parent_height: usize,
        parent_padding: usize,
        _parent_margin: usize,
        display: &Display,
    ) {
        let geometry = self.geometry_mut();

        geometry.margin = match geometry.user_margin {
            Size::Absolute(size) => size,
            Size::Relative(size) => ceil_f32(parent_height as f32 * size as f32 / 100.0) as usize,
            _ => 0,
        };

        geometry.padding = match geometry.user_padding {
            Size::Absolute(size) => size,
            Size::Relative(size) => ceil_f32(parent_height as f32 * size as f32 / 100.0) as usize,
            _ => 0,
        };

        let available_x = parent_x + parent_padding;
        let available_y = parent_y + parent_padding;
        let available_width = parent_width.saturating_sub(2 * parent_padding);
        let available_height = parent_height.saturating_sub(2 * parent_padding);

        geometry.x = match geometry.user_x {
            Size::Absolute(size) => available_x + geometry.margin + size,
            Size::Relative(size) => {
                available_x + geometry.margin + ceil_f32(available_width as f32 * size as f32 / 100.0) as usize
            }
            Size::FromRight(size) => parent_x + parent_width - parent_padding - size - geometry.margin,
            Size::FromLeft(size) => available_x + size + geometry.margin,
            _ => available_x + geometry.margin,
        };

        geometry.y = match geometry.user_y {
            Size::Absolute(size) => available_y + geometry.margin + size,
            Size::Relative(size) => {
                available_y + geometry.margin + ceil_f32(available_height as f32 * size as f32 / 100.0) as usize
            }
            Size::FromUp(size) => available_y + size + geometry.margin,
            Size::FromDown(size) => parent_y + parent_height - parent_padding - size - geometry.margin,
            _ => available_y + geometry.margin,
        };

        let max_width = available_width.saturating_sub(geometry.margin * 2);
        let max_height = available_height.saturating_sub(geometry.margin * 2);

        geometry.width = match geometry.user_width {
            Size::Absolute(size) => size.min(max_width),
            Size::Relative(size) => ceil_f32(max_width as f32 * size as f32 / 100.0) as usize,
            Size::Auto => max_width,
            _ => max_width,
        };

        geometry.height = match geometry.user_height {
            Size::Absolute(size) => size.min(max_height),
            Size::Relative(size) => ceil_f32(max_height as f32 * size as f32 / 100.0) as usize,
            Size::Auto => max_height,
            _ => max_height,
        };

        let widget_x = geometry.x;
        let widget_y = geometry.y;
        let widget_width = geometry.width;
        let widget_height = geometry.height;
        let widget_padding = geometry.padding;
        let _widget_margin = geometry.margin;

        match self {
            Widget::Frame { children, .. } => {
                match display {
                    Display::Grid { rows, cols } => {
                        if *rows == 0 || *cols == 0 { return; }
                        let content_width = widget_width.saturating_sub(widget_padding * 2);
                        let content_height = widget_height.saturating_sub(widget_padding * 2);
                        let cell_width = content_width / *cols;
                        let cell_height = content_height / *rows;

                        for (i, child) in children.iter_mut().enumerate() {
                            let row = i / *cols;
                            let col = i % *cols;
                            if row >= *rows { break; }

                            let child_parent_x = widget_x + widget_padding + col * cell_width;
                            let child_parent_y = widget_y + widget_padding + row * cell_height;

                            child.update_layout(child_parent_x, child_parent_y, cell_width, cell_height, 0, _widget_margin, &Display::None);
                        }
                    },
                    Display::Flex { direction, wrap } => {
                        let content_x = widget_x + widget_padding;
                        let content_y = widget_y + widget_padding;
                        let content_width = widget_width.saturating_sub(widget_padding * 2);
                        let content_height = widget_height.saturating_sub(widget_padding * 2);

                        let children_vec = core::mem::take(children);
                        let mut current_x = 0;
                        let mut current_y = 0;
                        let mut line_height = 0;
                        let mut line_width = 0;

                        for mut child in children_vec.into_iter() {
                            child.update_layout(content_x, content_y, content_width, content_height, 0, _widget_margin, &Display::None);
                            let child_geo = child.geometry();
                            let child_total_w = child_geo.width + child_geo.margin * 2;
                            let child_total_h = child_geo.height + child_geo.margin * 2;

                            match direction {
                                FlexDirection::Row => {
                                    if *wrap && current_x + child_total_w > content_width && current_x > 0 {
                                        current_x = 0;
                                        current_y += line_height;
                                        line_height = 0;
                                    }
                                    let child_x = content_x + current_x;
                                    let child_y = content_y + current_y;
                                    let avail_w = (content_width - current_x).min(child_total_w);
                                    let avail_h = content_height.saturating_sub(current_y);

                                    child.update_layout(child_x, child_y, avail_w, avail_h.min(child_total_h), 0, _widget_margin, &Display::None);

                                    current_x += child_total_w;
                                    line_height = line_height.max(child_total_h);
                                },
                                FlexDirection::Column => {
                                    if *wrap && current_y + child_total_h > content_height && current_y > 0 {
                                        current_y = 0;
                                        current_x += line_width;
                                        line_width = 0;
                                    }
                                    let child_x = content_x + current_x;
                                    let child_y = content_y + current_y;
                                    let avail_w = content_width.saturating_sub(current_x);
                                    let avail_h = (content_height - current_y).min(child_total_h);

                                    child.update_layout(child_x, child_y, avail_w.min(child_total_w), avail_h, 0, _widget_margin, &Display::None);

                                    current_y += child_total_h;
                                    line_width = line_width.max(child_total_w);
                                }
                            }
                            children.push(child);
                        }
                    },
                    Display::None => {
                        let _content_x = widget_x + widget_padding;
                        let _content_y = widget_y + widget_padding;
                        let content_width = widget_width.saturating_sub(widget_padding * 2);
                        let content_height = widget_height.saturating_sub(widget_padding * 2);
                        let children_vec = core::mem::take(children);
                        for mut child in children_vec.into_iter() {
                            child.update_layout(widget_x + widget_padding, widget_y + widget_padding, content_width, content_height, 0, _widget_margin, &Display::None);
                            children.push(child);
                        }
                    }
                }
            },
            _ => {}
        }
    }
}

use titanf::TrueTypeFont;
use crate::LinearGradient;

impl Widget {
    // ... existing methods ...
    pub fn draw(&self, framebuffer: &mut [u32], buffer_width: usize, font: &mut Option<TrueTypeFont>) {
        if buffer_width == 0 { return; }

        let geometry = self.geometry();

        match self {
            Widget::Frame { background, .. } => {
                crate::graphics::primitives::draw_background_style(
                    framebuffer,
                    buffer_width,
                    geometry.x + geometry.margin,
                    geometry.y + geometry.margin,
                    geometry.width,
                    geometry.height,
                    geometry.border_radius,
                    background
                );
            },
            Widget::Button { background, text, .. } => {
                crate::graphics::primitives::draw_background_style(
                    framebuffer,
                    buffer_width,
                    geometry.x + geometry.margin,
                    geometry.y + geometry.margin,
                    geometry.width,
                    geometry.height,
                    geometry.border_radius,
                    background
                );

                if let Some(font) = font {
                    if !text.text.is_empty() {
                        crate::graphics::primitives::draw_text(
                            framebuffer,
                            buffer_width,
                            geometry.x + geometry.margin + geometry.padding + 5,
                            geometry.y + geometry.margin + geometry.padding + 5,
                            &text.text,
                            font,
                            text.size as f32,
                            text.color
                        );
                    }
                }
            },
            Widget::Label { background, text, .. } => {
                // Only draw background if it's not fully transparent
                let should_draw_bg = match background {
                    BackgroundStyle::Solid(c) => c.a > 0,
                    BackgroundStyle::Gradient(g) => g.start_color.a > 0 || g.end_color.a > 0,
                };

                if should_draw_bg {
                    crate::graphics::primitives::draw_background_style(
                        framebuffer,
                        buffer_width,
                        geometry.x + geometry.margin,
                        geometry.y + geometry.margin,
                        geometry.width,
                        geometry.height,
                        geometry.border_radius,
                        background
                    );
                }

                if let Some(font) = font {
                    if !text.text.is_empty() {
                        crate::graphics::primitives::draw_text(
                            framebuffer,
                            buffer_width,
                            geometry.x + geometry.margin + geometry.padding,
                            geometry.y + geometry.margin + geometry.padding,
                            &text.text,
                            font,
                            text.size as f32,
                            text.color
                        );
                    }
                }
            },
            Widget::Canvas { framebuffer: widget_buffer, .. } => {
                if !widget_buffer.is_empty() {
                    for row in 0..geometry.height {
                        let dest_start = (geometry.y + row) * buffer_width + geometry.x;
                        let dest_end = dest_start + geometry.width;
                        let src_start = row * geometry.width;
                        let src_end = src_start + geometry.width;

                        if dest_end <= framebuffer.len() && src_end <= widget_buffer.len() {
                            framebuffer[dest_start..dest_end].copy_from_slice(&widget_buffer[src_start..src_end]);
                        }
                    }
                }
            }
        }
    }

    pub fn find_widget_at(&self, x: usize, y: usize) -> Option<WidgetId> {
        let geometry = self.geometry();
        if x >= geometry.x && x < geometry.x + geometry.width &&
           y >= geometry.y && y < geometry.y + geometry.height {
            
            if let Some(children) = self.get_children() {
                for child in children.iter().rev() {
                    if let Some(child_id) = child.find_widget_at(x, y) {
                        return Some(child_id);
                    }
                }
            }
            Some(geometry.id)
        } else {
            None
        }
    }

    pub fn find_widget_by_id_mut(&mut self, target_id: WidgetId) -> Option<&mut Widget> {
        if self.geometry().id == target_id {
            return Some(self);
        }
        if let Some(children) = self.get_children_mut() {
            for child in children {
                if let Some(found) = child.find_widget_by_id_mut(target_id) {
                    return Some(found);
                }
            }
        }
        None
    }
    
    pub fn find_widget_by_id(&self, target_id: WidgetId) -> Option<&Widget> {
        if self.geometry().id == target_id {
            return Some(self);
        }
        if let Some(children) = self.get_children() {
            for child in children {
                if let Some(found) = child.find_widget_by_id(target_id) {
                    return Some(found);
                }
            }
        }
        None
    }
}