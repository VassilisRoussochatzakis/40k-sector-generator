use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

/// Handle to a background job.
pub struct JobHandle<T> {
    pub id: String,
    pub revision: u64,
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
pub fn spawn_job<T, F>(
    id: &str,
    revision: u64,
    description: &str,
    ctx: egui::Context,
    f: F,
) -> JobHandle<T>
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
        ui_ctx: ctx.clone(),
    };

    thread::spawn(move || {
        let result = f(job_ctx);
        let _ = tx.send(result);
        ctx.request_repaint();
    });

    JobHandle {
        id: id.to_string(),
        revision,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn job_handle_carries_revision_and_cancel_flag() {
        let ctx = egui::Context::default();
        let handle = spawn_job("preview-gen", 42, "preview", ctx, |_| "done");

        assert_eq!(handle.revision, 42);
        assert!(!handle.is_cancelled());
        handle.cancel();
        assert!(handle.is_cancelled());
        assert_eq!(
            handle.receiver.recv_timeout(Duration::from_secs(1)),
            Ok("done")
        );
    }
}
