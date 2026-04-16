use crate::sync::Mutex;
use crate::task::TASK_MANAGER;

/// Dirty region accumulator. `full` means the entire screen needs recomposing.
/// Partial updates are unioned into a single bounding rect.
struct DirtyState {
    full: bool,
    pending: bool,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

impl DirtyState {
    const fn new() -> Self {
        DirtyState { full: false, pending: false, x: 0, y: 0, w: 0, h: 0 }
    }

    fn union(&mut self, x: i32, y: i32, w: u32, h: u32) {
        if self.full { return; }
        if !self.pending {
            self.x = x;
            self.y = y;
            self.w = w;
            self.h = h;
            self.pending = true;
        } else {
            let x2 = (self.x + self.w as i32).max(x + w as i32);
            let y2 = (self.y + self.h as i32).max(y + h as i32);
            self.x = self.x.min(x);
            self.y = self.y.min(y);
            self.w = (x2 - self.x).max(0) as u32;
            self.h = (y2 - self.y).max(0) as u32;
        }
    }

    fn set_full(&mut self) {
        self.full = true;
        self.pending = true;
    }

    fn take(&mut self) -> Option<RenderJob> {
        if !self.pending { return None; }
        if self.full {
            self.full = false;
            self.pending = false;
            Some(RenderJob::Full)
        } else {
            self.pending = false;
            Some(RenderJob::Rect(self.x, self.y, self.w, self.h))
        }
    }
}

enum RenderJob {
    Full,
    Rect(i32, i32, u32, u32),
}

static DIRTY: Mutex<DirtyState> = Mutex::new(DirtyState::new());
static RENDER_SEMAPHORE: std::sync::Semaphore = std::sync::Semaphore::new(0);

/// Queue a partial dirty region for async compositing.
pub fn mark_dirty(x: i32, y: i32, w: u32, h: u32) {
    DIRTY.lock().union(x, y, w, h);
    RENDER_SEMAPHORE.signal();
}

/// Queue a full-screen recompose (workspace switch, window add/remove, etc.).
/// Safe to call from any context — never acquires DISPLAY_SERVER.
pub fn mark_all_dirty() {
    DIRTY.lock().set_full();
    RENDER_SEMAPHORE.signal();
}

const RENDER_CPU: usize = 1;

extern "C" fn render_thread_main() {
    loop {
        RENDER_SEMAPHORE.wait();

        let job = DIRTY.lock().take();
        match job {
            Some(RenderJob::Full) => {
                let composer = crate::window_manager::composer::COMPOSER.read();
                composer.recompose_all();
            }
            Some(RenderJob::Rect(dx, dy, dw, dh)) => {
                let composer = crate::window_manager::composer::COMPOSER.read();
                composer.update_window_area_rect(dx, dy, dw, dh);
            }
            None => {}
        }
    }
}

pub fn init() {
    let mut tm = TASK_MANAGER.lock();
    if let Ok(tid) = tm.spawn_thread(0, render_thread_main as u64, 0, 0) {
        tm.pin_thread_to_cpu(tid, RENDER_CPU);
    }
}
