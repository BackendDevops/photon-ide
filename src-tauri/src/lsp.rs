//! Intelephense LSP bridge — JSON-RPC 2.0 over stdio.
//!
//! One `LspClient` per workspace root, spawned lazily in `open_project`.
//! The reader thread lives for the lifetime of the client; it routes
//! responses by request-id and fans `publishDiagnostics` notifications
//! directly to the Tauri "diagnostics" event so the UI picks them up
//! through the same listener it uses for native + PHPStan results.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{
    atomic::{AtomicI64, Ordering},
    mpsc, Arc,
};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use photon_core::FileDiagnostics;

// ── Public types ──────────────────────────────────────────────────────────────

pub struct LspClient {
    stdin:   Mutex<ChildStdin>,
    next_id: AtomicI64,
    /// Pending request-id → sync-channel sender for the response payload.
    pending: Arc<Mutex<HashMap<i64, mpsc::SyncSender<Value>>>>,
    #[allow(dead_code)]
    pub root: String,
}

// Safety: ChildStdin, AtomicI64, Arc<Mutex<...>>, String are all Send+Sync.
unsafe impl Send for LspClient {}
unsafe impl Sync for LspClient {}

// ── Core API ──────────────────────────────────────────────────────────────────

impl LspClient {
    /// Spawn `node <intelephense.js> --stdio`, perform LSP initialize/initialized
    /// handshake, and return a ready client. Non-blocking after return: the
    /// reader thread runs independently.
    pub fn spawn(root: &str, app: AppHandle) -> anyhow::Result<Self> {
        let binary = find_intelephense(root)
            .ok_or_else(|| anyhow::anyhow!(
                "Intelephense not found. Install: npm i -g intelephense"
            ))?;

        let mut child = Command::new("node")
            .arg(&binary)
            .arg("--stdio")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin  = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let pending: Arc<Mutex<HashMap<_, _>>> = Arc::default();
        let pending2 = pending.clone();

        std::thread::Builder::new()
            .name("photon-lsp-reader".into())
            .spawn(move || reader_loop(stdout, pending2, app))
            .map_err(|e| anyhow::anyhow!("spawn lsp reader: {e}"))?;

        let client = Self {
            stdin: Mutex::new(stdin),
            next_id: AtomicI64::new(1),
            pending,
            root: root.to_string(),
        };

        client.initialize(root)?;
        Ok(client)
    }

    // ── Lifecycle notifications ───────────────────────────────────────────────

    /// Notify LSP that a file was opened for the first time.
    pub fn did_open(&self, uri: &str, content: &str) {
        let _ = self.notify("textDocument/didOpen", json!({
            "textDocument": {
                "uri": uri,
                "languageId": "php",
                "version": 1,
                "text": content
            }
        }));
    }

    /// Notify LSP that a file's content changed (full-sync mode).
    pub fn did_change(&self, uri: &str, version: i32, content: &str) {
        let _ = self.notify("textDocument/didChange", json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": content }]
        }));
    }

    /// Notify LSP that a file was closed.
    #[allow(dead_code)]
    pub fn did_close(&self, uri: &str) {
        let _ = self.notify("textDocument/didClose", json!({
            "textDocument": { "uri": uri }
        }));
    }

    // ── Feature requests ─────────────────────────────────────────────────────

    /// Request hover info at (line, col) (1-based). Times out after 80ms so
    /// the hover command stays responsive even when Intelephense is indexing.
    pub fn hover(&self, uri: &str, line: u32, col: u32) -> Option<String> {
        let resp = self.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": {
                    "line":      line.saturating_sub(1),  // LSP is 0-based
                    "character": col.saturating_sub(1)
                }
            }),
            80,
        ).ok()?;

        extract_hover_markdown(&resp)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn initialize(&self, root: &str) -> anyhow::Result<()> {
        let uri = path_to_uri(root);
        let storage = std::path::Path::new(root)
            .join(".photon/lsp-cache")
            .to_string_lossy()
            .into_owned();

        // Block up to 10 s for the initialize response.
        self.request("initialize", json!({
            "rootUri": uri,
            "rootPath": root,
            "capabilities": {
                "textDocument": {
                    "hover": {
                        "dynamicRegistration": false,
                        "contentFormat": ["markdown", "plaintext"]
                    },
                    "publishDiagnostics": {
                        "relatedInformation": false
                    },
                    "synchronization": {
                        "dynamicRegistration": false,
                        "didSave": false
                    }
                },
                "workspace": { "workspaceFolders": true }
            },
            "initializationOptions": {
                "licenceKey": "",
                "clearCache": false,
                "storagePath": storage,
                "telemetry": { "enabled": false }
            },
            "workspaceFolders": [{
                "uri": uri,
                "name": std::path::Path::new(root)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "project".into())
            }]
        }), 10_000)?;

        // initialized notification — no response.
        self.notify("initialized", json!({}))?;
        Ok(())
    }

    fn notify(&self, method: &str, params: Value) -> anyhow::Result<()> {
        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "method":  method,
            "params":  params
        }))
    }

    fn request(&self, method: &str, params: Value, timeout_ms: u64) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::sync_channel::<Value>(1);
        self.pending.lock().insert(id, tx);
        self.send_raw(&json!({
            "jsonrpc": "2.0",
            "id":      id,
            "method":  method,
            "params":  params
        }))?;
        rx.recv_timeout(Duration::from_millis(timeout_ms))
            .map_err(|_| anyhow::anyhow!("LSP timeout after {timeout_ms}ms for {method}"))
    }

    fn send_raw(&self, msg: &Value) -> anyhow::Result<()> {
        let body = serde_json::to_string(msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        self.stdin.lock().write_all(frame.as_bytes())?;
        Ok(())
    }
}

// ── Reader thread ─────────────────────────────────────────────────────────────

fn reader_loop(
    stdout: std::process::ChildStdout,
    pending: Arc<Mutex<HashMap<i64, mpsc::SyncSender<Value>>>>,
    app: AppHandle,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        // Read the Content-Length header line.
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 {
            break; // process exited
        }
        let header = header.trim();
        if header.is_empty() {
            continue;
        }
        let len: usize = match header
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.parse().ok())
        {
            Some(n) => n,
            None => continue,
        };

        // Consume the blank separator line (\r\n).
        let mut blank = String::new();
        if reader.read_line(&mut blank).unwrap_or(0) == 0 {
            break;
        }

        // Read exactly `len` bytes.
        let mut body = vec![0u8; len];
        {
            use std::io::Read;
            if reader.read_exact(&mut body).is_err() {
                break;
            }
        }

        let msg: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(id) = msg["id"].as_i64() {
            // Route response to the waiting caller.
            if let Some(tx) = pending.lock().remove(&id) {
                let _ = tx.send(msg["result"].clone());
            }
        } else if let Some(method) = msg["method"].as_str() {
            // Server-initiated notification.
            handle_notification(method, &msg["params"], &app);
        }
    }
}

fn handle_notification(method: &str, params: &Value, app: &AppHandle) {
    if method == "textDocument/publishDiagnostics" {
        if let Some(batch) = lsp_diags_to_native(params) {
            let _ = app.emit("diagnostics", batch);
        }
    }
    // Intentionally ignore window/logMessage, $/progress, etc.
}

// ── Conversion helpers ────────────────────────────────────────────────────────

fn lsp_diags_to_native(params: &Value) -> Option<FileDiagnostics> {
    let uri  = params["uri"].as_str()?;
    let file = uri_to_path(uri);
    let diags = params["diagnostics"]
        .as_array()?
        .iter()
        .filter_map(|d| {
            let line    = d["range"]["start"]["line"].as_u64()? as u32 + 1;
            let col     = d["range"]["start"]["character"].as_u64()? as u32 + 1;
            let end_col = d["range"]["end"]["character"].as_u64()? as u32 + 1;
            let message = d["message"].as_str()?.to_string();
            let severity = match d["severity"].as_u64() {
                Some(1) => "error",
                Some(2) => "warning",
                _       => "info",
            };
            Some(photon_core::Diagnostic {
                line, col, end_col,
                message,
                severity: severity.into(),
            })
        })
        .collect();
    Some(FileDiagnostics { file, source: "lsp".into(), diagnostics: diags })
}

/// Extract the Markdown string from an LSP hover response.
/// Handles both the MarkupContent object and legacy array formats.
fn extract_hover_markdown(resp: &Value) -> Option<String> {
    // Most common: { contents: { kind: "markdown", value: "..." } }
    if let Some(v) = resp["contents"]["value"].as_str() {
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    // Older: { contents: "plain string" }
    if let Some(s) = resp["contents"].as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    // Array variant: { contents: [{ language, value }, ...] }
    if let Some(arr) = resp["contents"].as_array() {
        let joined: String = arr
            .iter()
            .filter_map(|item| {
                item["value"].as_str()
                    .or_else(|| item.as_str())
                    .filter(|s| !s.is_empty())
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        if !joined.is_empty() {
            return Some(joined);
        }
    }
    None
}

// ── Discovery ─────────────────────────────────────────────────────────────────

/// Find the Intelephense JS entry point.
/// Priority: project-local node_modules → global npm root.
pub fn find_intelephense(project_root: &str) -> Option<String> {
    // 1. Project-local (npm install --save-dev intelephense)
    let local = std::path::Path::new(project_root)
        .join("node_modules/intelephense/lib/intelephense.js");
    if local.exists() {
        return Some(local.to_string_lossy().into_owned());
    }
    // 2. Global npm root (npm i -g intelephense)
    if let Ok(out) = Command::new("npm").args(["root", "-g"]).output() {
        let npm_root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let global = std::path::Path::new(&npm_root)
            .join("intelephense/lib/intelephense.js");
        if global.exists() {
            return Some(global.to_string_lossy().into_owned());
        }
    }
    None
}

// ── URI utilities ─────────────────────────────────────────────────────────────

pub fn path_to_uri(path: &str) -> String {
    let p = path.replace('\\', "/");
    if p.starts_with('/') {
        format!("file://{p}")
    } else {
        format!("file:///{p}") // Windows: file:///C:/...
    }
}

fn uri_to_path(uri: &str) -> String {
    uri.strip_prefix("file:///")
        .or_else(|| uri.strip_prefix("file://"))
        .unwrap_or(uri)
        .to_string()
}
