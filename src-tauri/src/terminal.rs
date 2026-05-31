//! Integrated terminal (docs/02 §terminal).
//!
//! Real PTY sessions via `portable-pty`, one per terminal tab. Output is
//! streamed to the UI as Tauri events `term-data-<id>`; input/resize/kill come
//! back as commands. Multiple concurrent terminals are supported.

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

struct Term {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

#[derive(Default)]
pub struct Terminals {
    inner: Mutex<HashMap<String, Term>>,
    seq: Mutex<u64>,
}

fn default_shell() -> String {
    if cfg!(windows) {
        std::env::var("COMSPEC").unwrap_or_else(|_| "powershell.exe".into())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into())
    }
}

impl Terminals {
    pub fn spawn(
        &self,
        app: AppHandle,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<String, String> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;

        let mut cmd = CommandBuilder::new(default_shell());
        if let Some(dir) = cwd {
            if !dir.is_empty() {
                cmd.cwd(dir);
            }
        }
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

        let id = {
            let mut s = self.seq.lock().unwrap();
            *s += 1;
            format!("term-{}", *s)
        };

        // Reader thread → stream output to the UI.
        let id2 = id.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = app.emit(&format!("term-exit-{id2}"), ());
                        break;
                    }
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app.emit(&format!("term-data-{id2}"), data);
                    }
                }
            }
        });

        self.inner.lock().unwrap().insert(
            id.clone(),
            Term {
                writer,
                master: pair.master,
                child,
            },
        );
        Ok(id)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(t) = guard.get_mut(id) {
            t.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
            t.writer.flush().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let guard = self.inner.lock().unwrap();
        if let Some(t) = guard.get(id) {
            t.master
                .resize(PtySize {
                    rows: rows.max(1),
                    cols: cols.max(1),
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn kill(&self, id: &str) {
        if let Some(mut t) = self.inner.lock().unwrap().remove(id) {
            let _ = t.child.kill();
        }
    }
}
