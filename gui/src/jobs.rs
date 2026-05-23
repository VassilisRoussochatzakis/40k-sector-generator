use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

/// Handle to a background job.
pub struct JobHandle<T> {
    pub id: String,
    pub description: String,
    pub progress: Arc<Mutex<f32>>, // 0.0 to 1.0
    pub cancelled: Arc<AtomicBool>,
    pub receiver: Receiver<T>,
}

impl<T> JobHandle<T> {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn progress(&self) -> f32 {
        *self.progress.lock().unwrap()
    }
}

/// Helper to spawn a job and return a handle.
pub fn spawn_job<T, F>(id: &str, description: &str, ctx: egui::Context, f: F) -> JobHandle<T>
where
    T: Send + 'static,
    F: FnOnce(JobContext) -> T + Send + 'static,
{
    let (tx, rx) = channel();
    let progress = Arc::new(Mutex::new(0.0));
    let cancelled = Arc::new(AtomicBool::new(false));

    let job_ctx = JobContext {
        progress: progress.clone(),
        cancelled: cancelled.clone(),
        ui_ctx: ctx,
    };

    thread::spawn(move || {
        let result = f(job_ctx);
        let _ = tx.send(result);
    });

    JobHandle {
        id: id.to_string(),
        description: description.to_string(),
        progress,
        cancelled,
        receiver: rx,
    }
}

pub struct JobContext {
    progress: Arc<Mutex<f32>>,
    cancelled: Arc<AtomicBool>,
    ui_ctx: egui::Context,
}

impl JobContext {
    pub fn set_progress(&self, p: f32) {
        *self.progress.lock().unwrap() = p;
        self.ui_ctx.request_repaint();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
