import re

with open("kernel/src/window_manager/composer.rs", "r") as f:
    content = f.read()

# Replace layout_tree
old_layout_tree = """    fn layout_tree(&mut self, ws_idx: usize, node_idx: usize, rx: i32, ry: i32, rw: u32, rh: u32) {
        let node = self.workspaces[ws_idx].tree[node_idx];
        let spacing = self.spacing as u32;

        if let Some(win_idx) = node.leaf_window {
            let w = &mut self.workspaces[ws_idx].windows[win_idx];
            w.x = rx as i64;
            w.y = ry as i64;
            w.width = rw.max(1) as u64;
            w.height = rh.max(1) as u64;
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
    }"""

new_layout_tree = """    fn layout_tree(&mut self, ws_idx: usize, node_idx: usize, rx: i32, ry: i32, rw: u32, rh: u32) {
        let node = self.workspaces[ws_idx].tree[node_idx];
        let spacing = self.spacing as u32;

        if let Some(win_idx) = node.leaf_window {
            let w = &self.workspaces[ws_idx].windows[win_idx];
            let target_x = rx as i64;
            let target_y = ry as i64;
            let target_w = rw.max(1) as u64;
            let target_h = rh.max(1) as u64;
            
            // Only send resize event, DO NOT update w in kernel until app responds
            if w.x != target_x || w.y != target_y || w.width != target_w || w.height != target_h {
                let event = crate::window_manager::events::Event::Resize(
                    crate::window_manager::events::ResizeEvent {
                        wid: w.id as u32,
                        width: target_w as u32,
                        height: target_h as u32,
                        x: target_x as i32,
                        y: target_y as i32,
                    }
                );
                crate::window_manager::events::GLOBAL_EVENT_QUEUE.int_lock().add_event(event);
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
    }"""

content = content.replace(old_layout_tree, new_layout_tree)

with open("kernel/src/window_manager/composer.rs", "w") as f:
    f.write(content)
