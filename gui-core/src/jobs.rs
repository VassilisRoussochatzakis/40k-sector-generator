use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

/// Lets [`spawn_job`] turn a *worker panic* into a surfaced failure value of the
/// job's own result type, instead of letting the panic silently drop the sender
/// (which the receiver would only ever observe as `Disconnected`).
///
/// Implement it on a result enum that has a "failed" variant carrying a message
/// — return `Some(Self::Failed(message))`. Return `None` when the type can't
/// represent a panic as a value (e.g. a result whose failure variant needs more
/// context than a string, or a bare smoke-test payload); in that case the worker
/// thread drops the sender as before and the receiver sees `Disconnected`, with
/// the panic-hook crash note (see [`crate::diagnostics`]) as the breadcrumb.
///
/// Under the release profile (`panic = "abort"`) the `catch_unwind` in
/// `spawn_job` is a no-op — the process aborts before this runs — so this path
/// only surfaces failures in debug/unwinding builds; the panic hook is the
/// release fallback.
pub trait FromJobPanic: Sized {
    /// Build a failure value from a worker panic message, or `None` if this type
    /// can't represent one.
    fn from_job_panic(message: String) -> Option<Self>;
}

/// Bare payloads used by smoke tests and trivial jobs can't carry a failure, so
/// a panic in such a worker falls through to the `Disconnected` path.
impl FromJobPanic for &'static str {
    fn from_job_panic(_message: String) -> Option<Self> {
        None
    }
}

/// Handle to a background job.
pub struct JobHandle<T> {
    pub id: String,
    pub revision: u64,
    pub description: String,
    pub progress: Arc<AtomicU32>, // 0.0..=1.0, stored as f32 bits (lock-free)
    pub status: Arc<Mutex<Option<String>>>,
    pub cancelled: Arc<AtomicBool>,
    pub receiver: Receiver<T>,
}

impl<T> JobHandle<T> {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn progress(&self) -> f32 {
        f32::from_bits(self.progress.load(Ordering::Relaxed))
    }

    /// Latest live status line the worker posted via [`JobContext::set_status`]
    /// (e.g. a byte counter), or `None` if it hasn't posted one.
    pub fn status(&self) -> Option<String> {
        self.status.lock().unwrap().clone()
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
    T: FromJobPanic + Send + 'static,
    F: FnOnce(JobContext) -> T + Send + 'static,
{
    let (tx, rx) = channel();
    let progress = Arc::new(AtomicU32::new(0.0f32.to_bits()));
    let status = Arc::new(Mutex::new(None));
    let cancelled = Arc::new(AtomicBool::new(false));

    let job_ctx = JobContext {
        progress: progress.clone(),
        status: status.clone(),
        cancelled: cancelled.clone(),
        ui_ctx: ctx.clone(),
        last_repaint_pct: AtomicU32::new(u32::MAX),
    };

    thread::spawn(move || {
        // A worker panic must not silently drop the sender — the receiver would
        // then only ever see `Disconnected`, which a caller can't tell apart
        // from a vanished thread. Catch it and convert it into a surfaced
        // failure of `T` (when `T` can represent one). `AssertUnwindSafe` is
        // sound here: on the unwind path we never touch `f`'s captured state
        // again — we only build a fresh failure value from the panic message.
        // (Under release `panic = "abort"` this catch is a no-op; the
        // `diagnostics` panic hook is the fallback there.)
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| f(job_ctx)));
        match outcome {
            Ok(result) => {
                let _ = tx.send(result);
            }
            Err(payload) => {
                let message = panic_message(payload.as_ref());
                // If `T` can't represent the failure, drop the sender as before;
                // the receiver then observes `Disconnected`.
                if let Some(failed) = T::from_job_panic(message) {
                    let _ = tx.send(failed);
                }
            }
        }
        ctx.request_repaint();
    });

    JobHandle {
        id: id.to_string(),
        revision,
        description: description.to_string(),
        progress,
        status,
        cancelled,
        receiver: rx,
    }
}

/// Extract a human-readable message from a panic payload (a `catch_unwind`
/// `Err`, or a panic hook's [`std::panic::PanicHookInfo::payload`]). Panic
/// payloads are almost always `&str` (from `panic!("literal")`) or `String`
/// (from `panic!("{}", x)` / `.expect(format!(...))`); anything else degrades to
/// a placeholder. Shared with [`crate::diagnostics`]'s crash-note writer.
pub(crate) fn panic_message(payload: &dyn std::any::Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

pub struct JobContext {
    progress: Arc<AtomicU32>,
    status: Arc<Mutex<Option<String>>>,
    cancelled: Arc<AtomicBool>,
    ui_ctx: egui::Context,
    /// Rounded percent (0..=100) at which we last asked egui to repaint from a
    /// progress tick. We only repaint when this advances, so a tight worker loop
    /// (or several parallel workers) can't drive the UI redraw every tick.
    /// Atomic so `JobContext` stays `Sync` — parallel runners share `&JobContext`.
    last_repaint_pct: AtomicU32,
}

impl JobContext {
    pub fn set_progress(&self, p: f32) {
        self.progress.store(p.to_bits(), Ordering::Relaxed);
        // Throttle repaints to once per advancing whole percent: a worker can
        // call this in a tight loop (and parallel runners share one JobContext),
        // and egui would otherwise redraw every tick. At most ~101 repaints per
        // job; the terminal `request_repaint()` in `spawn_job` always flushes the
        // final state, so throttling here can't leave the UI stuck short of 100%.
        let pct = (p.clamp(0.0, 1.0) * 100.0) as u32;
        if self.last_repaint_pct.swap(pct, Ordering::Relaxed) != pct {
            self.ui_ctx.request_repaint();
        }
    }

    /// Post a live status line for the UI to display (e.g. a byte counter while
    /// a large `sector.json` streams to disk).
    pub fn set_status(&self, status: impl Into<String>) {
        *self.status.lock().unwrap() = Some(status.into());
        self.ui_ctx.request_repaint();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
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

    #[test]
    fn set_progress_round_trips_through_atomic_bits() {
        let ctx = egui::Context::default();
        let (checked_tx, checked_rx) = std::sync::mpsc::channel();
        let (progressed_tx, progressed_rx) = std::sync::mpsc::channel();
        let handle = spawn_job("progress-job", 1, "progress", ctx, move |jc| {
            jc.set_progress(0.42);
            progressed_tx.send(()).unwrap(); // progress is now stored
            checked_rx.recv_timeout(Duration::from_secs(2)).unwrap(); // hold until main reads
            "done"
        });

        progressed_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            (handle.progress() - 0.42).abs() < 1e-6,
            "progress() must read back the f32 stored via to_bits, got {}",
            handle.progress()
        );

        checked_tx.send(()).unwrap();
        assert_eq!(
            handle.receiver.recv_timeout(Duration::from_secs(1)),
            Ok("done")
        );
    }

    #[test]
    fn spawn_job_dispatch_returns_before_worker_finishes() {
        let ctx = egui::Context::default();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        let handle = spawn_job("export-smoke", 1, "export smoke", ctx, move |_| {
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            "done"
        });

        assert!(matches!(
            handle.receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));

        release_tx.send(()).unwrap();
        assert_eq!(
            handle.receiver.recv_timeout(Duration::from_secs(1)),
            Ok("done")
        );
    }

    /// A test result type that *can* represent a worker panic — mirrors the
    /// shape of the real `*JobResult` enums (`Done` / `Failed(String)`).
    // `Done` is kept for shape-parity with the real enums even though only the
    // panic path (`Failed`) is exercised here.
    #[allow(dead_code)]
    #[derive(Debug, PartialEq, Eq)]
    enum TestResult {
        Done(i32),
        Failed(String),
    }

    impl FromJobPanic for TestResult {
        fn from_job_panic(message: String) -> Option<Self> {
            Some(Self::Failed(message))
        }
    }

    #[test]
    fn worker_panic_becomes_surfaced_failed_result() {
        let ctx = egui::Context::default();
        // Silence the default hook's stderr print for this intentional panic;
        // restore it afterwards so other tests aren't affected.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let handle: JobHandle<TestResult> = spawn_job("panic-job", 7, "panicker", ctx, |_| {
            panic!("boom in worker")
        });
        let got = handle.receiver.recv_timeout(Duration::from_secs(1));
        std::panic::set_hook(prev);

        match got {
            Ok(TestResult::Failed(msg)) => assert!(
                msg.contains("boom in worker"),
                "failure should carry the panic message, got: {msg}"
            ),
            other => panic!("expected surfaced Failed, got {other:?}"),
        }
    }

    #[test]
    fn worker_panic_on_non_representable_type_disconnects() {
        let ctx = egui::Context::default();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        // `&'static str` returns `None` from `from_job_panic`, so a panic drops
        // the sender and the receiver observes `Disconnected` (unchanged).
        let handle: JobHandle<&'static str> =
            spawn_job("panic-str", 1, "panicker", ctx, |_| panic!("boom"));
        let got = handle.receiver.recv_timeout(Duration::from_secs(1));
        std::panic::set_hook(prev);

        assert_eq!(got, Err(std::sync::mpsc::RecvTimeoutError::Disconnected));
    }
}
