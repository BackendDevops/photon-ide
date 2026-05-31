//! Zero-config Xdebug debugger over DBGp (docs/19 — power tools).
//!
//! The IDE listens on TCP 9003; Xdebug connects to us. We configure breakpoints,
//! drive stepping, and emit break/stack/variable events to the UI. Minimal XML
//! parsing (no extra deps) — enough for local PHP/Herd debugging.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub struct DebugState {
    /// (absolute file path, 1-based line, optional condition expression).
    pub breakpoints: Mutex<Vec<(String, u32, Option<String>)>>,
    /// Channel to the live session thread; carries "verb\targs" (no `-i`).
    pub tx: Mutex<Option<Sender<String>>>,
    pub listening: Mutex<bool>,
}

/// Build `breakpoint_set` args for a line or conditional breakpoint.
pub fn breakpoint_args(abs: &str, line: u32, cond: Option<&str>) -> String {
    match cond {
        Some(c) if !c.trim().is_empty() => {
            format!("-t conditional -f file://{abs} -n {line} -- {}", base64_encode(c))
        }
        _ => format!("-t line -f file://{abs} -n {line}"),
    }
}

#[derive(serde::Serialize, Clone)]
pub struct StackFrame {
    pub file: String,
    pub line: u32,
    pub func: String,
}

#[derive(serde::Serialize, Clone)]
pub struct Variable {
    pub name: String,
    pub ty: String,
    pub value: String,
}

#[derive(serde::Serialize, Clone)]
struct BreakPayload {
    file: String,
    line: u32,
    stack: Vec<StackFrame>,
    vars: Vec<Variable>,
}

/// Begin listening for an Xdebug connection (idempotent).
pub fn listen(app: AppHandle, state: &DebugState) -> Result<(), String> {
    {
        let mut l = state.listening.lock().unwrap();
        if *l {
            return Ok(());
        }
        *l = true;
    }
    let breakpoints = state.breakpoints.lock().unwrap().clone();
    let (tx, rx) = channel::<String>();
    *state.tx.lock().unwrap() = Some(tx);

    std::thread::spawn(move || {
        let listener = match TcpListener::bind("127.0.0.1:9003") {
            Ok(l) => l,
            Err(e) => {
                let _ = app.emit("xdebug-error", format!("bind 9003: {e}"));
                return;
            }
        };
        let _ = app.emit("xdebug-status", "listening");
        if let Ok((stream, _)) = listener.accept() {
            session(app.clone(), stream, breakpoints, rx);
        }
        let _ = app.emit("xdebug-status", "stopped");
    });
    Ok(())
}

/// Send a DBGp command (`verb -i <id> [args]`) and read its response packet.
fn send_cmd(stream: &mut TcpStream, id: &mut u32, verb: &str, args: &str) -> Option<String> {
    *id += 1;
    let line = if args.is_empty() {
        format!("{verb} -i {id}\0")
    } else {
        format!("{verb} -i {id} {args}\0")
    };
    stream.write_all(line.as_bytes()).ok()?;
    read_packet(stream)
}

fn session(
    app: AppHandle,
    mut stream: TcpStream,
    breakpoints: Vec<(String, u32, Option<String>)>,
    rx: Receiver<String>,
) {
    let mut id = 0u32;

    // init packet
    if read_packet(&mut stream).is_none() {
        return;
    }
    let _ = app.emit("xdebug-status", "connected");
    let _ = send_cmd(&mut stream, &mut id, "feature_set", "-n max_depth -v 3");
    let _ = send_cmd(&mut stream, &mut id, "feature_set", "-n max_children -v 64");
    for (file, ln, cond) in &breakpoints {
        let _ = send_cmd(&mut stream, &mut id, "breakpoint_set", &breakpoint_args(file, *ln, cond.as_deref()));
    }

    // auto-run to the first breakpoint
    if let Some(resp) = send_cmd(&mut stream, &mut id, "run", "") {
        handle_stop(&app, &mut stream, &resp, &mut id);
    }

    // drive from frontend commands
    while let Ok(msg) = rx.recv() {
        let (verb, args) = msg.split_once('\t').unwrap_or((msg.as_str(), ""));
        if verb == "stop" {
            let _ = send_cmd(&mut stream, &mut id, "stop", "");
            break;
        }
        let resp = send_cmd(&mut stream, &mut id, verb, args);
        match verb {
            "run" | "step_into" | "step_over" | "step_out" => {
                if let Some(r) = &resp {
                    handle_stop(&app, &mut stream, r, &mut id);
                }
            }
            "property_get" => {
                if let Some(r) = &resp {
                    let _ = app.emit("xdebug-property", parse_vars(r));
                }
            }
            _ => {}
        }
    }
    let _ = app.emit("xdebug-status", "stopped");
}

/// React to a continuation response: emit break (with stack + vars) or end.
fn handle_stop(app: &AppHandle, stream: &mut TcpStream, resp: &str, id: &mut u32) {
    let status = attr(resp, "status").unwrap_or_default();
    if status == "stopping" || status == "stopped" {
        let _ = app.emit("xdebug-end", "");
        return;
    }
    if status != "break" {
        return;
    }
    // stack_get
    *id += 1;
    let stack_xml = write_read(stream, &format!("stack_get -i {id}\0")).unwrap_or_default();
    let stack = parse_stack(&stack_xml);
    let (file, line) = stack
        .first()
        .map(|f| (f.file.clone(), f.line))
        .unwrap_or_default();
    // context_get (locals)
    *id += 1;
    let ctx_xml = write_read(stream, &format!("context_get -i {id} -c 0\0")).unwrap_or_default();
    let vars = parse_vars(&ctx_xml);
    let _ = app.emit("xdebug-break", BreakPayload { file, line, stack, vars });
}

fn write_read(stream: &mut TcpStream, cmd: &str) -> Option<String> {
    stream.write_all(cmd.as_bytes()).ok()?;
    read_packet(stream)
}

/// DBGp framing: `<len>\0<xml>\0`.
fn read_packet(stream: &mut TcpStream) -> Option<String> {
    let mut len_buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        if byte[0] == 0 {
            break;
        }
        len_buf.push(byte[0]);
    }
    let len: usize = String::from_utf8_lossy(&len_buf).trim().parse().ok()?;
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).ok()?;
    let mut nul = [0u8; 1];
    let _ = stream.read(&mut nul);
    Some(String::from_utf8_lossy(&data).to_string())
}

fn attr(xml: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = xml.find(&key)? + key.len();
    let rest = &xml[i..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_stack(xml: &str) -> Vec<StackFrame> {
    let mut out = Vec::new();
    for chunk in xml.split("<stack ").skip(1) {
        let file = attr(chunk, "filename").unwrap_or_default().replace("file://", "");
        let line = attr(chunk, "lineno").and_then(|s| s.parse().ok()).unwrap_or(0);
        let func = attr(chunk, "where").unwrap_or_default();
        out.push(StackFrame { file, line, func });
    }
    out
}

fn parse_vars(xml: &str) -> Vec<Variable> {
    let mut out = Vec::new();
    for chunk in xml.split("<property ").skip(1) {
        let name = attr(chunk, "name").unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let ty = attr(chunk, "type").unwrap_or_default();
        // value sits between `>` and `</property>`; may be base64-encoded.
        let value = chunk
            .find('>')
            .map(|i| &chunk[i + 1..])
            .and_then(|r| r.find("</property>").map(|e| r[..e].to_string()))
            .map(|raw| decode_value(chunk, &raw))
            .unwrap_or_default();
        out.push(Variable { name, ty, value });
    }
    out
}

fn decode_value(chunk: &str, raw: &str) -> String {
    if attr(chunk, "encoding").as_deref() == Some("base64") {
        base64_decode(raw.trim()).unwrap_or_else(|| raw.trim().to_string())
    } else {
        raw.trim().to_string()
    }
}

/// Tiny standard base64 encoder (for conditional breakpoint expressions).
fn base64_encode(s: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = s.as_bytes();
    let mut out = String::new();
    for chunk in b.chunks(3) {
        let n = chunk.len();
        let b0 = chunk[0];
        let b1 = if n > 1 { chunk[1] } else { 0 };
        let b2 = if n > 2 { chunk[2] } else { 0 };
        out.push(T[(b0 >> 2) as usize] as char);
        out.push(T[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if n > 1 { T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char } else { '=' });
        out.push(if n > 2 { T[(b2 & 0x3f) as usize] as char } else { '=' });
    }
    out
}

/// Tiny standard base64 decoder (Xdebug encodes property values).
fn base64_decode(s: &str) -> Option<String> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut idx = [255u8; 256];
    for (i, &c) in T.iter().enumerate() {
        idx[c as usize] = i as u8;
    }
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = idx[c as usize];
        if v == 255 {
            continue;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(String::from_utf8_lossy(&out).to_string())
}
