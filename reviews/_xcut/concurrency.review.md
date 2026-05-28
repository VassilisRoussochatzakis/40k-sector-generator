---
unit_id: X04
crate: workspace (cross-cutting)
paths:
  - src/analysis/search.rs
  - gui-core/src/jobs.rs
  - builder/src/builder/file_watcher.rs
  - builder/src/builder/preview.rs
  - viewer/src/app/lifecycle.rs
  - viewer/src/app/export_ui.rs
  - viewer/src/editor/state.rs
loc_reviewed: ~600
reviewed_by: agent
health_score: 4
finding_counts: { critical: 0, high: 0, medium: 2, low: 3, nit: 2 }
top_risks:
  - "Detached worker threads in spawn_job — no JoinHandle, lost panics (F-X04-001)"
  - "Mutex poisoning unwrap on progress could escalate a worker panic to UI panic (F-X04-002)"
---

# Review: Concurrency cross-cutting sweep

## Summary

The workspace is single-runtime with no async surface whatsoever (confirmed by grep). The only true concurrency primitives in use are:

1. One `rayon::into_par_iter` site in `src/analysis/search.rs:1091-1123` for parallel candidate evaluation. Determinism is preserved via `IndexedParallelIterator::collect` into `Vec<Slot>`.
2. Two `std::thread::spawn` sites: `gui-core/src/jobs.rs:52` (generic background job, detached) and `builder/src/builder/file_watcher.rs:57` (polling watcher, properly joined on drop).
3. Two atomic flags (`AtomicBool` cancel signals) in the same two files. No `Arc<Mutex<T>>` of mutable domain data — the only `Arc<Mutex<>>` is `Arc<Mutex<f32>>` for progress reporting.

No `unsafe impl Send`/`Sync`, no global mutable state, no channel/lock primitives beyond `std::sync::mpsc` for the worker-to-UI plumbing. The data-parallel rayon site is correctly designed for byte-stable output (CLAUDE.md determinism invariant).

The two real findings worth attention: `spawn_job` discards the worker's `JoinHandle`, so worker panics evaporate silently; and `progress.lock().unwrap()` can escalate a poisoned mutex into a UI-thread panic, though the poisoning path is currently unreachable. Everything else is style/efficiency polish.

## Method

```bash
grep -rEn "async fn|\.await\b|tokio|futures::|async_std|smol" --include="*.rs" -- src gui-core builder viewer
# → 0 matches. Async surface confirmed zero.

grep -rEn "rayon::|par_iter|into_par_iter|par_bridge|par_extend" --include="*.rs" -- src gui-core builder viewer
# → 1 site, src/analysis/search.rs:1091, 1099.

grep -rEn "std::thread::spawn|thread::spawn|thread::Builder|JoinHandle|spawn_blocking" \
  --include="*.rs" -- src gui-core builder viewer
# → 2 sites: gui-core/src/jobs.rs:52, builder/src/builder/file_watcher.rs:57.

grep -rEn "Atomic[A-Z]|Ordering::" --include="*.rs" -- src gui-core builder viewer
# → cancel flags (AtomicBool) only; rest is `cmp::Ordering` for sort comparators.

grep -rEn "Arc<Mutex|Arc<RwLock|parking_lot::|RwLock<" --include="*.rs" -- src gui-core builder viewer
# → 2 sites use `Arc<Mutex<f32>>` for progress; no Arc<Mutex<T>> over mutable domain data.

grep -rEn "unsafe impl Send|unsafe impl Sync" --include="*.rs"
# → 0 matches.

grep -rEn "static mut|static [A-Z_]+: " --include="*.rs" -- src
# → only `dhat::Alloc` in a profile binary. No shared mutable state.

grep -rEn "ThreadPool|RAYON_NUM_THREADS|build_global" --include="*.rs" -- src
# → 0 matches. Rayon uses default global pool.
```

## Findings

### F-X04-001 — [MEDIUM] [Concurrency / Resource] `spawn_job` detaches the worker thread; panics evaporate
- **Location:** `gui-core/src/jobs.rs:52-56`
- **Category:** Concurrency / Resource management
- **Confidence:** High
- **Blast radius:** All background work — preview generation and PNG exports in viewer and builder.
- **Problem:** `spawn_job` calls `thread::spawn(move || ...)` and immediately discards the returned `JoinHandle`. Three consequences:
  1. A panic inside the worker closure (e.g. an unchecked `unwrap` in `sectorforge::generation::generate_with_progress_and_cancel` or `analytics::analyze`) is silently swallowed by `std::thread`. The UI sees `TryRecvError::Disconnected` and reports "preview worker disconnected" — generic and uninformative.
  2. There's no way for the host to `join()` the worker on shutdown; if a worker is still running when the egui event loop exits, the process tears down a live thread mid-work. Today this is fine because the OS reclaims everything, but it's a footgun for any future cleanup hook (e.g. flushing tracing buffers).
  3. The `JobHandle` carries a `cancelled: AtomicBool` but no way to *wait* for the cancel to take effect. A second `spawn_job` for the same logical work can race with a still-draining previous worker.
- **Why it matters:** Worker panic = silent failure visible only as a vague channel-disconnected message. Across many features (preview, multiple export kinds), the user just sees "worker disconnected" with no breadcrumb.
- **Evidence:** Read of `gui-core/src/jobs.rs:42-66`. Channel-disconnect fallback in callers: `builder/src/builder/preview.rs:139-143`, `viewer/src/app/lifecycle.rs:264-268`, `viewer/src/app/export_ui.rs:114-117`.
- **Suggested fix:** Store the `JoinHandle` on `JobHandle`, and on drop (or on `cancel()` followed by a subsequent draining call) attempt a non-blocking join. Even just keeping the handle would let a future debug build emit a `thread::Builder::name(...)` + a custom panic hook that logs the worker thread's panic payload before the channel-disconnect signal reaches the UI.
  ```rust
  // gui-core/src/jobs.rs
  pub struct JobHandle<T> {
      // ...existing fields
      join: Option<std::thread::JoinHandle<()>>,
  }

  let join = std::thread::Builder::new()
      .name(format!("sf-job:{id}"))
      .spawn(move || {
          let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(job_ctx)));
          match result {
              Ok(value) => { let _ = tx.send(value); }
              Err(payload) => {
                  // Drop tx => receiver sees Disconnected, but log first.
                  let msg = panic_message(&payload);
                  log::error!("background job {id} panicked: {msg}");
              }
          }
          ctx.request_repaint();
      })
      .expect("OS thread spawn");
  ```
  At minimum, store the handle so the host can `join()` it during a clean teardown.
- **Effort:** S
- **Risk of fix:** Low — only adds an `Option<JoinHandle<()>>` to the public struct. `JoinHandle` is `Send`+`Sync` so no trait bounds shift.

### F-X04-002 — [MEDIUM] [Concurrency / Panics] `progress.lock().unwrap()` propagates poisoning into the UI thread
- **Location:** `gui-core/src/jobs.rs:26`, `gui-core/src/jobs.rs:76`
- **Category:** Concurrency / Panic surface
- **Confidence:** Medium (poisoning is currently unreachable but the contract is fragile)
- **Blast radius:** UI thread reading progress every frame; worker thread writing progress.
- **Problem:** Both reads (`*self.progress.lock().unwrap()`) and writes (`*self.progress.lock().unwrap() = p`) propagate poisoning. Today no code panics *while holding* this `Mutex<f32>` so poisoning is unreachable, but the invariant relies on nobody adding logic inside a `let mut g = self.progress.lock().unwrap(); ... ;` block in the future. If F-X04-001 is fixed (so worker panics get caught with `catch_unwind`), this stays safe. If not, a future change that locks and panics inside the lock scope would poison the mutex and crash the UI thread on the next read.
- **Why it matters:** `f32` progress is intrinsically a cell — `AtomicU32` (with `f32::to_bits` / `f32::from_bits`) sidesteps poisoning entirely and removes one allocation (`Arc<Mutex<...>>` → `Arc<AtomicU32>`). Same cost, fewer footguns.
- **Evidence:** Read of `gui-core/src/jobs.rs:7-83`. The viewer's `EditorState::handle_export_job` calls `job.progress()` every frame (`viewer/src/app/export_ui.rs:123`), so a poisoned mutex reaches the UI hot path immediately.
- **Suggested fix:** Replace `Arc<Mutex<f32>>` with `Arc<AtomicU32>` storing `f32::to_bits(p)`:
  ```rust
  pub struct JobHandle<T> {
      // ...
      pub progress: Arc<AtomicU32>,   // f32 bits
  }
  impl<T> JobHandle<T> {
      pub fn progress(&self) -> f32 {
          f32::from_bits(self.progress.load(Ordering::Relaxed))
      }
  }
  impl JobContext {
      pub fn set_progress(&self, p: f32) {
          self.progress.store(p.to_bits(), Ordering::Relaxed);
          self.ui_ctx.request_repaint();
      }
  }
  ```
  `Relaxed` is fine — progress is a monotonic display value with no happens-before requirement against other state.
- **Effort:** S
- **Risk of fix:** Low — pure local refactor in `gui-core`. Same public method shape.

### F-X04-003 — [LOW] [Concurrency / Performance] `FileWatcher::poll_loop` clones `baseline` every tick
- **Location:** `builder/src/builder/file_watcher.rs:97`
- **Category:** Concurrency / Allocation in a polling thread
- **Confidence:** High
- **Blast radius:** One background thread, once per second.
- **Problem:** `for (rel, last) in baseline.clone().iter()` clones the entire `BTreeMap<String, SystemTime>` on every poll tick to satisfy the borrow checker (the loop body inserts into `baseline`). For the documented "O(dozens) of TOML files" this is cheap, but the clone is unnecessary work and an unnecessary allocation pattern.
- **Why it matters:** The contract is "cheap polling" (per the module doc at `file_watcher.rs:6-7`). The clone undermines that contract slightly and grows linearly with project size if a future project carries more tracked files (catalogs, fragments, etc.).
- **Evidence:** Read of `file_watcher.rs:86-126`.
- **Suggested fix:** Buffer the changes into a small `Vec` during iteration, then apply after the loop:
  ```rust
  let mut updates: Vec<(String, SystemTime)> = Vec::new();
  for (rel, last) in baseline.iter() {
      if cancel.load(Ordering::Acquire) { return; }
      let abs = root.join(rel);
      let Ok(meta) = std::fs::metadata(Path::new(abs.as_str())) else { continue };
      let Ok(now) = meta.modified() else { continue };
      if now > *last {
          updates.push((rel.clone(), now));
      }
  }
  for (rel, now) in updates {
      baseline.insert(rel.clone(), now);
      if tx.send(FileChange { rel_path: rel, mtime: now }).is_err() { return; }
  }
  ```
- **Effort:** S
- **Risk of fix:** Low — same observable behaviour, deterministic key order (still BTreeMap iteration).

### F-X04-004 — [LOW] [Concurrency / Atomics] `gui-core::jobs` uses `Ordering::SeqCst` where `Acquire`/`Release` would do
- **Location:** `gui-core/src/jobs.rs:18`, `gui-core/src/jobs.rs:22`, `gui-core/src/jobs.rs:81`
- **Category:** Concurrency / Atomics
- **Confidence:** High
- **Blast radius:** Performance only; correctness unchanged.
- **Problem:** The cancel flag uses `SeqCst` for both the writer (`store`) and the readers (`load`). `SeqCst` imposes a total order across all atomics on all threads — a strictly stronger guarantee than this code needs, and disproportionately expensive on architectures with weak memory models (ARM). The flag is a single bit with no other atomics it needs to be ordered against; `Release` for the store and `Acquire` for the loads is the textbook pattern and is what `builder/src/builder/file_watcher.rs:79,94,98,119` already uses.
- **Why it matters:** Inconsistency between the two cancel-flag implementations (one uses `Release`/`Acquire`, the other `SeqCst`) and a tiny but real cost on ARM. Worker checks the flag in tight inner loops via `is_cancelled()`.
- **Evidence:** Read of `gui-core/src/jobs.rs:1-83` and `builder/src/builder/file_watcher.rs:21-126`.
- **Suggested fix:** Change to `Ordering::Release` on store and `Ordering::Acquire` on loads in `gui-core/src/jobs.rs`. Matches the convention in `file_watcher.rs`. If F-X04-002 also lands, the new atomic-backed progress field can use `Relaxed`.
- **Effort:** S
- **Risk of fix:** Low.

### F-X04-005 — [LOW] [Concurrency / Cleanup] Dead variable `shutdown_check` in `poll_loop`
- **Location:** `builder/src/builder/file_watcher.rs:93`, `builder/src/builder/file_watcher.rs:123`
- **Category:** Concurrency / dead code
- **Confidence:** High
- **Blast radius:** Cosmetic — clippy `let_underscore_lock` doesn't apply, but `dead_code`/`unused_assignments` would catch it.
- **Problem:** `let mut shutdown_check = Duration::from_millis(0)` is incremented on each sleep slice but never read. Looks like a leftover from an aborted "force a poll after N ms" design.
- **Why it matters:** Future maintainer will assume it serves a purpose and either preserve dead semantics or remove it without understanding the intent. Either outcome is worse than removing it now.
- **Evidence:** Read of `file_watcher.rs:86-126`.
- **Suggested fix:** Delete the declaration and the `shutdown_check += tick / 10;` line. If the original intent (force a periodic reload regardless of mtime) is still wanted, gate it on a real `if shutdown_check >= some_threshold` block.
- **Effort:** S
- **Risk of fix:** Low.

### F-X04-006 — [NIT] [Concurrency / Documentation] `run_search` rayon site doesn't document its determinism contract on the public docstring
- **Location:** `src/analysis/search.rs:1049-1058` (the `run_search` doc), `src/analysis/search.rs:1083-1090` (the explanation lives in an internal comment)
- **Category:** Documentation
- **Confidence:** High
- **Blast radius:** Future modification of this function might lose determinism without anyone noticing on PR review.
- **Problem:** The function-level docstring on `run_search` says only "Deterministic search over candidate seeds." The actual mechanism that *makes* it deterministic — `IndexedParallelIterator::collect` into `Vec<Slot>` preserves order, so the first-pass winner picked from sequential iteration of the collected slots is the lowest `n` — lives in an inline comment at lines 1083-1090. CLAUDE.md treats determinism as a public guarantee, so the explanation belongs on the `///` doc.
- **Why it matters:** A future contributor refactoring this loop (e.g. switching to `try_for_each` or breaking early on the first pass) could silently lose the "lowest-n winner equals sequential winner" guarantee, breaking golden tests after the fact rather than at PR-review time.
- **Evidence:** Read of `search.rs:1047-1163`.
- **Suggested fix:** Move the key sentences from the inline `FIX.txt §13` comment up into the `///` block, e.g. add a `# Determinism` section explaining that order-preserving collect + sequential first-pass selection yields the same winner as the sequential equivalent.
- **Effort:** S
- **Risk of fix:** Low — pure doc edit.

### F-X04-007 — [NIT] [Concurrency / Observability] Background thread spawns are not named
- **Location:** `gui-core/src/jobs.rs:52`, `builder/src/builder/file_watcher.rs:57`
- **Category:** Observability
- **Confidence:** High
- **Blast radius:** Debugging — thread names show in `top -H`, `gdb`, `dhat`, panic backtraces.
- **Problem:** Both spawns use `std::thread::spawn` rather than `std::thread::Builder::new().name("...").spawn(...)`. When a worker panics or stalls, profiling tools show only "Thread #3" and operators can't distinguish "preview worker" from "file watcher" from "PNG export".
- **Why it matters:** Trivial when only two thread kinds exist, but workers are spawned per-export and per-preview-revision, so a real flame graph can have N anonymous workers active at once.
- **Evidence:** Read of both spawn sites.
- **Suggested fix:** Use `thread::Builder::new().name(format!("sf-job:{id}"))` in `spawn_job`, and `thread::Builder::new().name("sf-file-watcher")` in `FileWatcher::spawn`. Folds naturally into F-X04-001's `Builder::spawn` change.
- **Effort:** S
- **Risk of fix:** Low — `Builder::spawn` returns `io::Result<JoinHandle>` instead of an unwrapped handle; convert with `.expect("OS thread")` to match the current panic-on-spawn-failure semantics.

## Rubric coverage (concurrency-scoped — no findings noted explicitly)

- **3.5 Concurrency & async (async sub-category):** Confirmed zero async surface across `src`, `gui-core`, `builder`, `viewer`. No findings.
- **3.5 Concurrency & async (rayon sub-category):** One site (`src/analysis/search.rs:1091-1123`). Pure functional, uses `IndexedParallelIterator::collect` so order is preserved (determinism invariant satisfied). No shared mutable state. Closure can return `Slot::Skipped` instead of panicking. See F-X04-006 (doc nit).
- **3.5 Concurrency & async (locks):** No `Arc<RwLock<T>>` anywhere. The only `Arc<Mutex<T>>` is `Arc<Mutex<f32>>` for progress, addressed in F-X04-002. No lock-ordering concerns (only one lock kind exists).
- **3.5 Concurrency & async (atomics):** Two `AtomicBool` cancel flags, one inconsistent ordering choice (F-X04-004). No memory-ordering bugs.
- **3.5 Concurrency & async (channels):** `std::sync::mpsc` used for worker→UI plumbing. All consumers handle `TryRecvError::Disconnected` explicitly (`preview.rs:139`, `lifecycle.rs:264`, `export_ui.rs:114`). No `unwrap` on receive. No findings.
- **3.5 Concurrency & async (Send/Sync):** No `unsafe impl Send`/`Sync`. All shared types are stdlib-derived. No findings.
- **3.5 Concurrency & async (`static mut` / globals):** Only `dhat::Alloc` in a profile-only binary. No findings.
- **3.5 Concurrency & async (thread cleanup):** `FileWatcher::drop` joins. `JobHandle` does not — see F-X04-001.

## Summary of suggested fixes

- F-X04-001 — MEDIUM — Store `JoinHandle` + `catch_unwind` in `spawn_job` so worker panics surface as named errors, not silent channel disconnects — S / Low
- F-X04-002 — MEDIUM — Replace `Arc<Mutex<f32>>` progress with `Arc<AtomicU32>` (f32 bits, Relaxed) to remove the mutex-poisoning path — S / Low
- F-X04-003 — LOW — In `FileWatcher::poll_loop`, buffer updates into a `Vec` instead of cloning `baseline` every tick — S / Low
- F-X04-004 — LOW — Switch `gui-core::jobs` cancel-flag atomics from `SeqCst` to `Release`/`Acquire` for consistency with `file_watcher` — S / Low
- F-X04-005 — LOW — Delete dead `shutdown_check` variable in `poll_loop` — S / Low
- F-X04-006 — NIT — Promote the determinism explanation from inline comment to the `///` doc on `run_search` — S / Low
- F-X04-007 — NIT — Name background threads via `thread::Builder::new().name(...)` for debuggability — S / Low
