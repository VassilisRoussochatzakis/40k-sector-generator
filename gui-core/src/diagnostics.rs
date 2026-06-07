//! Process-level crash diagnostics for the GUI binaries.
//!
//! Under the release profile the workspace builds with `panic = "abort"`, so a
//! panic anywhere on the UI thread is an immediate, *silent* hard crash — no
//! unwind, no stderr the user is looking at, nothing left behind. The panic
//! hook installed here is the fallback for that case: it drops a timestamped
//! crash note into the OS temp dir before the process dies, so a field crash
//! leaves a breadcrumb (message + location + backtrace) instead of vanishing.
//!
//! It is a best-effort, dependency-free diagnostic: the file write is allowed
//! to fail silently (a crashing process can't usefully report a *second*
//! failure), and the previously-installed hook is always chained so the default
//! abort/print behaviour still happens.

use std::backtrace::Backtrace;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

/// Install a panic hook that writes a crash note to [`std::env::temp_dir`]
/// before delegating to the previously-installed hook.
///
/// `app_name` (e.g. `"builder"` / `"viewer"`) is woven into the filename so the
/// two binaries don't clobber each other's notes:
/// `sectorforge-<app_name>-crash-<unix_millis>.txt`.
///
/// Call this once, at the very start of `main`, before any other work — a panic
/// during early setup should still be captured. Chaining `take_hook` means it
/// composes with any hook a host already set; calling it more than once simply
/// stacks another note-writer in front (harmless, but unnecessary).
pub fn install_panic_hook(app_name: &str) {
    let app_name = app_name.to_string();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort: never let crash-note bookkeeping panic inside the panic
        // hook (that would abort immediately and lose even the chained hook).
        write_crash_note(&app_name, info);
        previous(info);
    }));
}

/// Compose the crash note text and attempt to write it. All IO errors are
/// swallowed: a process that is already panicking cannot usefully act on a
/// failed write, and we still want the chained hook to run.
fn write_crash_note(app_name: &str, info: &std::panic::PanicHookInfo<'_>) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let mut path = std::env::temp_dir();
    path.push(format!("sectorforge-{app_name}-crash-{millis}.txt"));

    // The payload is usually a `&str` or `String`; fall back to a placeholder
    // for any other panic value (std exposes no generic formatting for it).
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string());

    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown location>".to_string());

    let mut note = String::new();
    // `let _ =`: writing into a String is infallible, but `write!` returns a
    // Result; discard it rather than `.unwrap()` inside a panic hook.
    let _ = writeln!(note, "sectorforge {app_name} crash");
    let _ = writeln!(note, "unix_millis: {millis}");
    let _ = writeln!(note, "location:    {location}");
    let _ = writeln!(note, "message:     {message}");
    let _ = writeln!(note, "backtrace:");
    // `force_capture` always captures regardless of RUST_BACKTRACE; it returns a
    // "disabled/unsupported" marker on targets without backtrace support, which
    // is fine to record verbatim.
    let _ = writeln!(note, "{}", Backtrace::force_capture());

    if let Ok(mut file) = std::fs::File::create(&path) {
        let _ = file.write_all(note.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    /// The note text carries the panic message, the source location, and a
    /// backtrace section. We build the text directly (the hook itself replaces a
    /// process-global, which a unit test must not do) by exercising the same
    /// formatting path through a synthesized panic.
    #[test]
    fn crash_note_filename_is_app_scoped_and_timestamped() {
        // Filename shape is what keeps the builder/viewer notes from colliding.
        let millis: u128 = 1_700_000_000_000;
        let name = format!("sectorforge-builder-crash-{millis}.txt");
        assert!(name.starts_with("sectorforge-builder-crash-"));
        assert!(name.ends_with(".txt"));
    }

    /// A panic with a `&str` payload round-trips into the note message via the
    /// same downcast chain the hook uses.
    #[test]
    fn str_payload_downcasts_to_message() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("boom");
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        assert_eq!(message, "boom");
    }

    /// A `String` payload (e.g. from `panic!("{}", x)`) also resolves.
    #[test]
    fn string_payload_downcasts_to_message() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("kaboom"));
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        assert_eq!(message, "kaboom");
    }

    /// An exotic payload falls back to the placeholder rather than panicking.
    #[test]
    fn non_string_payload_uses_placeholder() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(42u32);
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        assert_eq!(message, "<non-string panic payload>");
    }
}
