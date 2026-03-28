use crate::debugln;
use crate::window_manager::display::{DISPLAY_SERVER, VIRTIO_ACTIVE};
use crate::window_manager::window::{Items, Window, NULL_WINDOW};
use alloc::vec::Vec;
use alloc::vec;

pub static mut CLICKED_WINDOW_ID: usize = 0;

#[derive(Clone, Copy)]
pub struct TilingNode {
    pub is_active: bool,
    pub split_horizontal: bool,
    pub left_child: Option<usize>,
    pub right_child: Option<usize>,
    pub parent: Option<usize>,
    pub leaf_window: Option<usize>,
}

impl TilingNode {
    pub const fn new() -> Self {
        TilingNode {
            is_active: false,
            split_horizontal: true,
            left_child: None,
            right_child: None,
            parent: None,
            leaf_window: None,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Workspace {
    pub windows: [Window; 16],
    pub tree: [TilingNode; 32], // 16 leaves + 15 internal nodes
    pub root: Option<usize>,
}

impl Workspace {
    pub const fn new() -> Self {
        Workspace {
            windows: [NULL_WINDOW; 16],
            tree: [TilingNode::new(); 32],
            root: None,
        }
    }

    pub fn alloc_node(&mut self) -> Option<usize> {
        for i in 0..32 {
            if !self.tree[i].is_active {
                self.tree[i] = TilingNode::new();
                self.tree[i].is_active = true;
                return Some(i);
            }
        }
        None
    }
}

pub struct Composer {
    pub workspaces: [Workspace; 5],
    pub active_workspace: usize,
    pub wallpaper: Window,
    pub taskbar: Window,
    pub spacing: usize,
}

pub static mut COMPOSER: Composer = Composer {
    workspaces: [Workspace::new(); 5],
    active_workspace: 0,
    wallpaper: NULL_WINDOW,
    taskbar: NULL_WINDOW,
    spacing: 10,
};

impl Composer {
    pub fn switch_workspace(&mut self, index: usize) {
        if index < 5 && index != self.active_workspace {
            self.active_workspace = index;
            self.recompose_all();
        }
    }

    fn get_taskbar_rect(&self) -> (i32, i32, u32, u32) {
        let (sw, sh) = unsafe {
            (
                (*(&raw mut DISPLAY_SERVER)).width as u32,
                (*(&raw mut DISPLAY_SERVER)).height as u32,
            )
        };

        if self.taskbar.w_type == Items::Null { return (0, 0, 0, 0); }

        let tw = self.taskbar.width as u32;
        let th = self.taskbar.height as u32;

        if self.taskbar.x == 0 && self.taskbar.y == 0 {
            if tw > th { return (0, 0, sw, th); } else { return (0, 0, tw, sh); }
        } else if self.taskbar.y == 0 { return (self.taskbar.x as i32, 0, tw, sh); }
        else if self.taskbar.x == 0 { return (0, self.taskbar.y as i32, sw, th); }

        (self.taskbar.x as i32, self.taskbar.y as i32, tw, th)
    }

    fn get_available_desktop(&self) -> (i32, i32, u32, u32) {
        let (sw, sh) = unsafe {
            (
                (*(&raw mut DISPLAY_SERVER)).width as u32,
                (*(&raw mut DISPLAY_SERVER)).height as u32,
            )
        };
        let (tx, ty, tw, th) = self.get_taskbar_rect();
        if tw == 0 && th == 0 { return (0, 0, sw, sh); }

        if ty == 0 && tw == sw { return (0, th as i32, sw, sh.saturating_sub(th)); }
        else if tx == 0 && th == sh { return (tw as i32, 0, sw.saturating_sub(tw), sh); }
        else if ty == (sh.saturating_sub(th)) as i32 && tw == sw { return (0, 0, sw, sh.saturating_sub(th)); }
        else if tx == (sw.saturating_sub(tw)) as i32 && th == sh { return (0, 0, sw.saturating_sub(tw), sh); }

        (0, 0, sw, sh)
    }

    fn layout_tree(&mut self, ws_idx: usize, node_idx: usize, rx: i32, ry: i32, rw: u32, rh: u32) {
        let node = self.workspaces[ws_idx].tree[node_idx];
        let spacing = self.spacing as u32;

        if let Some(win_idx) = node.leaf_window {
            let target_x = rx as i64;
            let target_y = ry as i64;
            let target_w = rw.max(1) as u64;
            let target_h = rh.max(1) as u64;

            let (needs_event, pid, wid) = {
                let w = &mut self.workspaces[ws_idx].windows[win_idx];
                let changed = w.x != target_x || w.y != target_y || w.width != target_w || w.height != target_h;
                
                // Authoritatively store the intended tiled size in prev fields
                w.prev_x = target_x;
                w.prev_y = target_y;
                w.prev_width = target_w;
                w.prev_height = target_h;
                
                (changed, w.pid, w.id)
            };
            
            if needs_event {
                let event = crate::window_manager::events::Event::Resize(
                    crate::window_manager::events::ResizeEvent {
                        wid: wid as u32,
                        width: target_w as u32,
                        height: target_h as u32,
                        x: target_x as i32,
                        y: target_y as i32,
                    }
                );
                
                let tm = crate::task::TASK_MANAGER.int_lock();
                if !crate::window_manager::events::GLOBAL_EVENT_QUEUE.int_lock().push_to_process(&*tm, pid, event) {
                    crate::window_manager::events::GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
                }
            }
            return;
        }

        let l_idx = node.left_child.unwrap();
        let r_idx = node.right_child.unwrap();

        if node.split_horizontal {
            let half_h = (rh.saturating_sub(spacing)) / 2;
            self.layout_tree(ws_idx, l_idx, rx, ry, rw, half_h);
            self.layout_tree(ws_idx, r_idx, rx, ry + (half_h + spacing) as i32, rw, rh.saturating_sub(half_h + spacing));
        } else {
            let half_w = (rw.saturating_sub(spacing)) / 2;
            self.layout_tree(ws_idx, l_idx, rx, ry, half_w, rh);
            self.layout_tree(ws_idx, r_idx, rx + (half_w + spacing) as i32, ry, rw.saturating_sub(half_w + spacing), rh);
        }
    }

    pub fn retile_workspace(&mut self, ws_idx: usize) {
        let (ax, ay, aw, ah) = self.get_available_desktop();
        let spacing = self.spacing as i32;

        if let Some(root) = self.workspaces[ws_idx].root {
            let rx = ax + spacing;
            let ry = ay + spacing;
            let rw = (aw as i32).saturating_sub(spacing * 2).max(0) as u32;
            let rh = (ah as i32).saturating_sub(spacing * 2).max(0) as u32;

            self.layout_tree(ws_idx, root, rx, ry, rw, rh);
        }
    }

    fn find_leaf_for_window(&self, ws_idx: usize, win_idx: usize) -> Option<usize> {
        for i in 0..32 {
            if self.workspaces[ws_idx].tree[i].is_active && self.workspaces[ws_idx].tree[i].leaf_window == Some(win_idx) {
                return Some(i);
            }
        }
        None
    }

    fn remove_node(&mut self, ws_idx: usize, node_idx: usize) {
        let parent_opt = self.workspaces[ws_idx].tree[node_idx].parent;
        
        if let Some(parent) = parent_opt {
            let p_node = self.workspaces[ws_idx].tree[parent];
            let sibling = if p_node.left_child == Some(node_idx) { p_node.right_child } else { p_node.left_child };
            
            let grandparent_opt = p_node.parent;
            if let Some(grandparent) = grandparent_opt {
                let gp_node = &mut self.workspaces[ws_idx].tree[grandparent];
                if gp_node.left_child == Some(parent) {
                    gp_node.left_child = sibling;
                } else {
                    gp_node.right_child = sibling;
                }
            } else {
                self.workspaces[ws_idx].root = sibling;
            }
            if let Some(s) = sibling {
                self.workspaces[ws_idx].tree[s].parent = grandparent_opt;
            }
            self.workspaces[ws_idx].tree[parent].is_active = false;
        } else {
            self.workspaces[ws_idx].root = None;
        }
        self.workspaces[ws_idx].tree[node_idx].is_active = false;
    }

    pub fn copy_window(&mut self, id: u64) {
        let ws = &self.workspaces[self.active_workspace];
        let screen_w = unsafe { (*(&raw mut DISPLAY_SERVER)).width as u32 };
        let screen_h = unsafe { (*(&raw mut DISPLAY_SERVER)).height as u32 };

        let mut fullscreen_id = 0;
        for i in 0..16 {
            let w = &ws.windows[i];
            if w.w_type == Items::Window && w.x <= 0 && w.y <= 0 && w.width as u32 >= screen_w && w.height as u32 >= screen_h {
                fullscreen_id = w.id;
                break;
            }
        }

        if fullscreen_id != 0 && fullscreen_id != id {
            return; // Suppress background updates when a fullscreen window is active
        }

        if id == self.wallpaper.id && self.wallpaper.w_type != Items::Null {
            if fullscreen_id == 0 {
                unsafe {
                    let ds = &mut *(&raw mut DISPLAY_SERVER);
                    ds.copy_to_db(self.wallpaper.width as u32, self.wallpaper.height as u32, self.wallpaper.get_active_buffer() as usize, self.wallpaper.x as i32, self.wallpaper.y as i32, None, self.wallpaper.treat_as_transparent);
                }
            }
            return;
        }
        if id == self.taskbar.id && self.taskbar.w_type != Items::Null {
            if fullscreen_id == 0 {
                unsafe {
                    let ds = &mut *(&raw mut DISPLAY_SERVER);
                    ds.copy_to_db(self.taskbar.width as u32, self.taskbar.height as u32, self.taskbar.get_active_buffer() as usize, self.taskbar.x as i32, self.taskbar.y as i32, None, self.taskbar.treat_as_transparent);
                }
            }
            return;
        }

        for i in 0..16 {
            if id == ws.windows[i].id && ws.windows[i].w_type != Items::Null {
                let is_fullscreen = fullscreen_id == id;
                let border_color = if ws.windows[i].w_type == Items::Window && !is_fullscreen {
                    unsafe { if ws.windows[i].id == CLICKED_WINDOW_ID as u64 { Some(0xFFFFFFFF) } else { Some(0xFF9070FF) } }
                } else { None };
                
                unsafe {
                    let ds = &mut *(&raw mut DISPLAY_SERVER);
                    ds.copy_to_db(ws.windows[i].width as u32, ws.windows[i].height as u32, ws.windows[i].get_active_buffer() as usize, ws.windows[i].x as i32, ws.windows[i].y as i32, border_color, if is_fullscreen { false } else { ws.windows[i].treat_as_transparent });
                }
            }
        }
    }

    pub fn copy_window_clipped(&mut self, _id: u64, _clip_w: u32, _clip_h: u32) {}

    pub fn copy_window_fb(&mut self, _id: u64) {}

    pub fn find_window(&mut self, x: usize, y: usize) -> Option<&mut Window> {
        let mx = x as i64;
        let my = y as i64;

        if self.taskbar.w_type != Items::Null {
            if mx >= self.taskbar.x && mx <= (self.taskbar.x + self.taskbar.width as i64) && my >= self.taskbar.y && my <= (self.taskbar.y + self.taskbar.height as i64) {
                return Some(&mut self.taskbar);
            }
        }

        let ws = &mut self.workspaces[self.active_workspace];
        let mut top_z = u64::MAX;
        let mut top_idx = 16;
        for i in 0..16 {
            if ws.windows[i].w_type != Items::Null {
                if mx >= ws.windows[i].x && mx <= (ws.windows[i].x + ws.windows[i].width as i64) && my >= ws.windows[i].y && my <= (ws.windows[i].y + ws.windows[i].height as i64) {
                    if ws.windows[i].z <= top_z {
                        top_z = ws.windows[i].z;
                        top_idx = i;
                    }
                }
            }
        }
        if top_idx < 16 { return Some(&mut ws.windows[top_idx]); }
        None
    }

    pub fn find_window_id(&mut self, id: u64) -> Option<&mut Window> {
        if self.wallpaper.id == id && self.wallpaper.w_type != Items::Null { return Some(&mut self.wallpaper); }
        if self.taskbar.id == id && self.taskbar.w_type != Items::Null { return Some(&mut self.taskbar); }

        for w in 0..5 {
            for i in 0..16 {
                if self.workspaces[w].windows[i].id == id && self.workspaces[w].windows[i].w_type != Items::Null {
                    return Some(&mut self.workspaces[w].windows[i]);
                }
            }
        }
        None
    }

    pub fn focus_window(&mut self, id: u64) {
        let mut target_idx = None;
        let mut target_ws = 0;
        for w in 0..5 {
            for i in 0..16 {
                if self.workspaces[w].windows[i].id == id && self.workspaces[w].windows[i].w_type != Items::Null {
                    target_idx = Some(i);
                    target_ws = w;
                    break;
                }
            }
        }

        if let Some(idx) = target_idx {
            let wtype = self.workspaces[target_ws].windows[idx].w_type;
            if wtype == Items::Bar || wtype == Items::Popup || wtype == Items::Wallpaper { return; }

            self.workspaces[target_ws].windows[idx].z = 1;
            for i in 0..16 {
                if i == idx { continue; }
                if self.workspaces[target_ws].windows[i].w_type != Items::Null {
                    self.workspaces[target_ws].windows[i].z = self.workspaces[target_ws].windows[i].z.saturating_add(1);
                }
            }
            self.recompose_all();
        }
    }

    pub fn check_id(&self, _rng_seed: u64) -> u64 {
        static mut NEXT_ID: u64 = 1;
        unsafe { let id = NEXT_ID; NEXT_ID += 1; id }
    }

    pub fn add_window(&mut self, mut w: Window) -> u64 {
        w.id = self.check_id(w.buffer as u64);
        w.prev_x = 0; w.prev_y = 0; w.prev_width = 0; w.prev_height = 0;

        if w.w_type == Items::Wallpaper {
            w.z = 255; w.transparent = false; w.treat_as_transparent = false; w.can_move = false; w.can_resize = false;
            self.wallpaper = w;
            self.recompose_all();
            return w.id;
        } else if w.w_type == Items::Bar {
            w.z = 0; w.can_move = false; w.can_resize = false;
            self.taskbar = w;
            self.recompose_all();
            return w.id;
        } else if w.w_type == Items::Popup {
            w.z = 0; w.can_move = false; w.can_resize = false;
        } else {
            w.z = 1;
            w.can_move = true; w.can_resize = true;
        }

        let ws_idx = self.active_workspace;
        let mut inserted_idx = 16;
        for i in 0..16 {
            if self.workspaces[ws_idx].windows[i].w_type == Items::Null {
                self.workspaces[ws_idx].windows[i] = w;
                inserted_idx = i;
                break;
            }
        }

        if inserted_idx == 16 { return w.id; }

        if w.w_type == Items::Window {
            if true { // w.width == 0 || w.height == 0
                crate::debugln!("[WINDOW_SERVER] TILING WID {}", w.id);
                // Hide window in workspace until it responds to the first tiling resize
                self.workspaces[ws_idx].windows[inserted_idx].width = 0;
                self.workspaces[ws_idx].windows[inserted_idx].height = 0;
                
                let leaf_id = self.workspaces[ws_idx].alloc_node().unwrap();
                self.workspaces[ws_idx].tree[leaf_id].leaf_window = Some(inserted_idx);

                let root = self.workspaces[ws_idx].root;
                if root.is_none() {
                    self.workspaces[ws_idx].root = Some(leaf_id);
                } else {
                    let mut split_node_idx = None;
                    let active_id = unsafe { CLICKED_WINDOW_ID } as u64;
                    if active_id != 0 {
                        for i in 0..16 {
                            if self.workspaces[ws_idx].windows[i].id == active_id {
                                split_node_idx = self.find_leaf_for_window(ws_idx, i);
                                break;
                            }
                        }
                    }

                    if split_node_idx.is_none() {
                        let mut last_id = 0;
                        for i in 0..16 {
                            if i != inserted_idx && self.workspaces[ws_idx].windows[i].w_type == Items::Window {
                                if self.find_leaf_for_window(ws_idx, i).is_some() {
                                    last_id = i;
                                }
                            }
                        }
                        split_node_idx = self.find_leaf_for_window(ws_idx, last_id);
                    }

                    if let Some(target_leaf) = split_node_idx {
                        let parent = self.workspaces[ws_idx].tree[target_leaf].parent;
                        let new_internal = self.workspaces[ws_idx].alloc_node().unwrap();
                        
                        let parent_horiz = parent.map_or(true, |p| self.workspaces[ws_idx].tree[p].split_horizontal);
                        self.workspaces[ws_idx].tree[new_internal].split_horizontal = !parent_horiz;
                        self.workspaces[ws_idx].tree[new_internal].left_child = Some(target_leaf);
                        self.workspaces[ws_idx].tree[new_internal].right_child = Some(leaf_id);
                        self.workspaces[ws_idx].tree[new_internal].parent = parent;

                        self.workspaces[ws_idx].tree[target_leaf].parent = Some(new_internal);
                        self.workspaces[ws_idx].tree[leaf_id].parent = Some(new_internal);

                        if let Some(p) = parent {
                            if self.workspaces[ws_idx].tree[p].left_child == Some(target_leaf) {
                                self.workspaces[ws_idx].tree[p].left_child = Some(new_internal);
                            } else {
                                self.workspaces[ws_idx].tree[p].right_child = Some(new_internal);
                            }
                        } else {
                            self.workspaces[ws_idx].root = Some(new_internal);
                        }
                    }
                }
                
                self.retile_workspace(ws_idx);

            } else {
                let (ax, ay, aw, ah) = self.get_available_desktop();
                self.workspaces[ws_idx].windows[inserted_idx].x = ax as i64 + (aw as i64 - w.width as i64) / 2;
                self.workspaces[ws_idx].windows[inserted_idx].y = ay as i64 + (ah as i64 - w.height as i64) / 2;
            }
        }

        self.recompose_all();
        w.id
    }

    pub fn resize_window(&mut self, w: Window) {
        crate::debugln!("[WINDOW_SERVER] RESIZING {} AT {} X {}", w.id, w.width, w.height);
        if w.id == self.wallpaper.id {
            self.wallpaper.buffer = w.buffer; self.wallpaper.back_buffer = w.back_buffer; self.wallpaper.flipped = w.flipped;
            self.wallpaper.width = w.width; self.wallpaper.height = w.height;
            self.update_window_area_rect(0, 0, w.width as u32, w.height as u32);
            return;
        }
        if w.id == self.taskbar.id {
            self.taskbar.buffer = w.buffer; self.taskbar.back_buffer = w.back_buffer; self.taskbar.flipped = w.flipped;
            self.taskbar.width = w.width; self.taskbar.height = w.height;
            self.update_window_area_rect(0, 0, self.taskbar.width as u32, self.taskbar.height as u32);
            return;
        }
        
        for ws in 0..5 {
            for i in 0..16 {
                if w.id == self.workspaces[ws].windows[i].id {
                    // If window is tiled, it MUST match the tiling target stored in prev_*
                    if let Some(_leaf) = self.find_leaf_for_window(ws, i) {
                        let target = &self.workspaces[ws].windows[i];
                        if w.width != target.prev_width || w.height != target.prev_height || w.x != target.prev_x || w.y != target.prev_y {
                            // Suppress resizes that don't match the tiling layout (e.g. initial app sync)
                            // This eliminates the "flash" at original size during spawn.
                            return;
                        }
                    }

                    let old_x = self.workspaces[ws].windows[i].x; let old_y = self.workspaces[ws].windows[i].y;
                    let old_w = self.workspaces[ws].windows[i].width; let old_h = self.workspaces[ws].windows[i].height;

                    self.workspaces[ws].windows[i].buffer = w.buffer; self.workspaces[ws].windows[i].back_buffer = w.back_buffer; self.workspaces[ws].windows[i].flipped = w.flipped;
                    self.workspaces[ws].windows[i].width = w.width; self.workspaces[ws].windows[i].height = w.height;
                    self.workspaces[ws].windows[i].x = w.x; self.workspaces[ws].windows[i].y = w.y;
                    self.workspaces[ws].windows[i].transparent = w.transparent; self.workspaces[ws].windows[i].treat_as_transparent = w.treat_as_transparent;

                    if ws == self.active_workspace {
                        let min_x = old_x.min(w.x) as i32; let min_y = old_y.min(w.y) as i32;
                        let max_x = (old_x + old_w as i64).max(w.x + w.width as i64) as i32; let max_y = (old_y + old_h as i64).max(w.y + w.height as i64) as i32;
                        self.update_window_area_rect(min_x, min_y, (max_x - min_x) as u32, (max_y - min_y) as u32);
                    }
                    
                    self.retile_workspace(ws);
                    return;
                }
            }
        }
    }

    pub fn remove_window(&mut self, wid: u64) {
        if self.wallpaper.id == wid { self.wallpaper = NULL_WINDOW; self.recompose_all(); return; }
        if self.taskbar.id == wid { self.taskbar = NULL_WINDOW; self.recompose_all(); return; }

        for ws in 0..5 {
            for i in 0..16 {
                if self.workspaces[ws].windows[i].id == wid {
                    self.workspaces[ws].windows[i] = NULL_WINDOW;
                    
                    if let Some(leaf_idx) = self.find_leaf_for_window(ws, i) {
                        self.remove_node(ws, leaf_idx);
                        if ws == self.active_workspace {
                            self.retile_workspace(ws);
                            self.recompose_all();
                        }
                    } else if ws == self.active_workspace {
                        self.recompose_all();
                    }
                    return;
                }
            }
        }
    }

    pub fn update_window_area_rect(&mut self, dirty_x: i32, dirty_y: i32, dirty_w: u32, dirty_h: u32) {
        unsafe { (*(&raw mut DISPLAY_SERVER)).mark_dirty(dirty_x, dirty_y, dirty_w, dirty_h); }
        self.recompose_area(dirty_x, dirty_y, dirty_w, dirty_h);
        unsafe {
            let ds = &mut *(&raw mut DISPLAY_SERVER);
            if VIRTIO_ACTIVE { ds.copy(); } else { ds.present_rect(dirty_x, dirty_y, dirty_w, dirty_h); }
        }
    }

    pub fn recompose_area(&mut self, dirty_x: i32, dirty_y: i32, dirty_w: u32, dirty_h: u32) {
        self.recompose_area_except(dirty_x, dirty_y, dirty_w, dirty_h, 0);
    }

    pub fn recompose_area_except(&mut self, dirty_x: i32, dirty_y: i32, dirty_w: u32, dirty_h: u32, ignore_id: u64) {
        unsafe {
            let display_server = &mut *(&raw mut DISPLAY_SERVER);
            if display_server.double_buffer != 0 {
                let db_ptr = display_server.double_buffer as *mut u32;
                let pitch_u32 = (display_server.pitch / 4) as usize;
                let height = display_server.height as i32;
                let width = display_server.width as i32;
                
                let ws = &self.workspaces[self.active_workspace];

                // FAST PATH: Fullscreen override
                // If there is a window occupying the entire screen, do 1:1 streaming
                let mut fullscreen_win = None;
                for i in 0..16 {
                    let w = &ws.windows[i];
                    if w.w_type == Items::Window && w.x <= 0 && w.y <= 0 && w.width as i32 >= width && w.height as i32 >= height {
                        fullscreen_win = Some(w);
                        break; // Only need one
                    }
                }

                if let Some(fw) = fullscreen_win {
                    if fw.id != ignore_id {
                        display_server.copy_to_db_clipped(fw.width as u32, fw.height as u32, fw.get_active_buffer() as usize, fw.x as i32, fw.y as i32, dirty_x, dirty_y, dirty_w, dirty_h, None, false);
                    }
                    return; // DO NOT render anything else
                }

                let start_x = dirty_x.max(0);
                let start_y = dirty_y.max(0);
                let end_x = (dirty_x + dirty_w as i32).min(width);
                let end_y = (dirty_y + dirty_h as i32).min(height);

                if end_x > start_x && end_y > start_y {
                    if self.wallpaper.w_type != Items::Null && self.wallpaper.id != ignore_id {
                        display_server.copy_to_db_clipped(self.wallpaper.width as u32, self.wallpaper.height as u32, self.wallpaper.get_active_buffer() as usize, self.wallpaper.x as i32, self.wallpaper.y as i32, dirty_x, dirty_y, dirty_w, dirty_h, None, self.wallpaper.treat_as_transparent);
                    } else {
                        for y in start_y..end_y {
                            let row_offset = y as usize * pitch_u32;
                            let row_ptr = db_ptr.add(row_offset + start_x as usize);
                            for x in 0..(end_x - start_x) as usize {
                                *row_ptr.add(x) = 0xFF333333;
                            }
                        }
                    }
                }

                let mut indices = vec![];
                for i in 0..16 { if ws.windows[i].w_type != Items::Null && ws.windows[i].id != ignore_id { indices.push(i); } }
                indices.sort_by_key(|&i| ws.windows[i].z);
                
                for i in indices.iter().rev() {
                    let w = &ws.windows[*i];
                    let border_color = if w.w_type == Items::Window { if w.id == CLICKED_WINDOW_ID as u64 { Some(0xFFFFFFFF) } else { Some(0xFF9070FF) } } else { None };
                    display_server.copy_to_db_clipped(w.width as u32, w.height as u32, w.get_active_buffer() as usize, w.x as i32, w.y as i32, dirty_x, dirty_y, dirty_w, dirty_h, border_color, w.treat_as_transparent);
                }

                if self.taskbar.w_type != Items::Null && self.taskbar.id != ignore_id {
                    display_server.copy_to_db_clipped(self.taskbar.width as u32, self.taskbar.height as u32, self.taskbar.get_active_buffer() as usize, self.taskbar.x as i32, self.taskbar.y as i32, dirty_x, dirty_y, dirty_w, dirty_h, None, self.taskbar.treat_as_transparent);
                }
            }
        }
    }

    pub fn recompose_all(&mut self) {
        let (sw, sh) = unsafe { let ds = &mut *(&raw mut DISPLAY_SERVER); (ds.width as u32, ds.height as u32) };
        self.update_window_area_rect(0, 0, sw, sh);
    }

    pub fn recompose_except(&mut self, except_id: u64) {
        let (sw, sh) = unsafe { let ds = &mut *(&raw mut DISPLAY_SERVER); (ds.width as u32, ds.height as u32) };
        self.recompose_area_except(0, 0, sw, sh, except_id);
        unsafe {
            let ds = &mut *(&raw mut DISPLAY_SERVER);
            if VIRTIO_ACTIVE { ds.copy(); } else { ds.present_rect(0, 0, sw, sh); }
        }
    }

    pub fn get_window_at(&mut self, x: usize, y: usize) -> u64 {
        let mx = x as i64;
        let my = y as i64;

        if self.taskbar.w_type != Items::Null {
            if mx >= self.taskbar.x && mx < self.taskbar.x + self.taskbar.width as i64 && my >= self.taskbar.y && my < self.taskbar.y + self.taskbar.height as i64 {
                return self.taskbar.id;
            }
        }

        let ws = &self.workspaces[self.active_workspace];
        let mut top_z = u64::MAX;
        let mut top_id = 0;

        for w in ws.windows.iter() {
            if w.w_type != Items::Null {
                if mx >= w.x && mx < w.x + w.width as i64 && my >= w.y && my < w.y + w.height as i64 {
                    if w.z <= top_z {
                        top_z = w.z;
                        top_id = w.id;
                    }
                }
            }
        }

        if top_id != 0 { return top_id; }
        
        if self.wallpaper.w_type != Items::Null {
            if mx >= self.wallpaper.x && mx < self.wallpaper.x + self.wallpaper.width as i64 && my >= self.wallpaper.y && my < self.wallpaper.y + self.wallpaper.height as i64 {
                return self.wallpaper.id;
            }
        }

        0
    }

    pub fn handle_mouse_click(&mut self, x: usize, y: usize) {
        let id = self.get_window_at(x, y);
        if id != 0 { self.focus_window(id); }
    }
}
