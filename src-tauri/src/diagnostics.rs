//! Background diagnostics worker.
//!
//! Receives (file, content) jobs from the Tauri command layer, runs native
//! photon-core inspections synchronously (fast path, <5ms), then optionally
//! invokes PHPStan as a subprocess (slow path, ~100-600ms). Each path emits
//! a separate `"diagnostics"` Tauri event so the UI can update incrementally.
//!
//! PHPStan is strictly optional — the IDE degrades gracefully when it is not
//! installed. Detection is attempted once per project root and cached.

use photon_core::FileDiagnostics;
use std::collections::HashMap;
use std::sync::{
    atomic::AtomicU64,
    mpsc::{self, SyncSender, TrySendError},
    Arc,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// A diagnostic job for one file buffer.
pub struct DiagJob {
    pub file: String,
    pub content: String,
    pub project_root: String,
    /// Monotonically increasing counter. A newer job for the same file
    /// supersedes an older one — the worker skips stale jobs.
    pub gen: u64,
}

/// Shared generation counter, incremented by the Tauri command layer on every
/// submission so the worker can detect stale jobs without locking.
pub type GenCounter = Arc<AtomicU64>;

/// Lightweight handle for enqueuing diagnostic jobs from Tauri commands.
#[derive(Clone)]
pub struct DiagnosticsWorker {
    tx: SyncSender<DiagJob>,
    pub gen: GenCounter,
}

impl DiagnosticsWorker {
    /// Spawn the background worker thread and return a handle to it.
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::sync_channel::<DiagJob>(256);
        let gen = Arc::new(AtomicU64::new(0));
        std::thread::Builder::new()
            .name("photon-diag".into())
            .spawn(move || worker_loop(rx, app))
            .expect("spawn diagnostics worker");
        Self { tx, gen }
    }

    /// Enqueue a job. Non-blocking — silently drops if the channel is full,
    /// which only happens when saves arrive faster than PHPStan can process.
    pub fn submit(&self, job: DiagJob) {
        match self.tx.try_send(job) {
            Ok(_) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

// ── Worker ────────────────────────────────────────────────────────────────────

fn worker_loop(rx: mpsc::Receiver<DiagJob>, app: AppHandle) {
    // Per-file: latest generation seen, so we can skip superseded jobs.
    let mut latest: HashMap<String, u64> = HashMap::new();
    // Per-project-root: whether PHPStan binary was found (None = not yet tried).
    let mut stan_available: HashMap<String, bool> = HashMap::new();

    // Debounce: coalesce a burst (auto-save + format + lint trigger) into one
    // batch. 250ms matches the frontend's debounce so they fire together.
    let debounce = Duration::from_millis(250);

    while let Ok(first) = rx.recv() {
        // Collect everything that arrives in the debounce window, keeping only
        // the newest job per file.
        let mut batch: HashMap<String, DiagJob> = HashMap::new();
        batch.insert(first.file.clone(), first);

        let deadline = Instant::now() + debounce;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            match rx.recv_timeout(left) {
                Ok(job) => {
                    batch.insert(job.file.clone(), job);
                }
                Err(_) => break,
            }
        }

        for (file, job) in batch {
            // Skip if a newer job arrived for this file after batch collection.
            let current = latest.entry(file.clone()).or_insert(0);
            if job.gen < *current {
                continue;
            }
            *current = job.gen;

            // ── Fast path: native photon-core inspections (~2ms) ─────────
            let native = run_native(&job.file, &job.content, &app);
            let _ = app.emit(
                "diagnostics",
                FileDiagnostics {
                    file: file.clone(),
                    source: "photon".into(),
                    diagnostics: native,
                },
            );

            // ── Slow path: PHPStan subprocess (~100-600ms) ────────────────
            // Only attempt when we know (or haven't yet checked) it's present.
            let available = stan_available
                .entry(job.project_root.clone())
                .or_insert_with(|| phpstan_binary(&job.project_root).is_some());

            if *available {
                match run_phpstan(&file, &job.project_root) {
                    Some(diags) if !diags.is_empty() => {
                        let _ = app.emit(
                            "diagnostics",
                            FileDiagnostics {
                                file: file.clone(),
                                source: "phpstan".into(),
                                diagnostics: diags,
                            },
                        );
                    }
                    None => {
                        // Binary disappeared — mark unavailable until restart.
                        *available = false;
                    }
                    _ => {}
                }
            }
        }
    }
}

// ── Native inspections ────────────────────────────────────────────────────────

fn run_native(
    file: &str,
    content: &str,
    _app: &AppHandle,
) -> Vec<photon_core::Diagnostic> {
    if !file.ends_with(".php") {
        return vec![];
    }
    photon_core::inspect::inspect_file(content)
}

// ── PHPStan subprocess ────────────────────────────────────────────────────────

/// Resolve the PHPStan binary for `project_root`: vendor-local first, then PATH.
fn phpstan_binary(root: &str) -> Option<String> {
    let vendor = std::path::Path::new(root).join("vendor/bin/phpstan");
    if vendor.exists() {
        return Some(vendor.to_string_lossy().into_owned());
    }
    // Check PATH
    if std::process::Command::new("phpstan")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Some("phpstan".into());
    }
    None
}

fn run_phpstan(file: &str, root: &str) -> Option<Vec<photon_core::Diagnostic>> {
    let binary = phpstan_binary(root)?;

    let out = std::process::Command::new(&binary)
        .args([
            "analyse",
            "--no-progress",
            "--error-format=json",
            "--level=5",
            file,
        ])
        .current_dir(root)
        // PHPStan exits 1 when errors found — that's expected, not a failure.
        .output()
        .ok()?;

    parse_phpstan_json(&out.stdout)
}

#[derive(serde::Deserialize)]
struct StanOutput {
    files: HashMap<String, StanFile>,
}

#[derive(serde::Deserialize)]
struct StanFile {
    messages: Vec<StanMessage>,
}

#[derive(serde::Deserialize)]
struct StanMessage {
    message: String,
    line: u32,
    #[serde(default)]
    tip: Option<String>,
}

fn parse_phpstan_json(raw: &[u8]) -> Option<Vec<photon_core::Diagnostic>> {
    let out: StanOutput = serde_json::from_slice(raw).ok()?;
    let diags = out
        .files
        .into_values()
        .flat_map(|f| f.messages)
        .map(|m| {
            let msg = match m.tip {
                Some(tip) if !tip.is_empty() => format!("{} 💡 {}", m.message, tip),
                _ => m.message,
            };
            photon_core::Diagnostic {
                line: m.line,
                col: 1,
                end_col: u32::MAX,
                message: msg,
                severity: "error".into(),
            }
        })
        .collect();
    Some(diags)
}
