//! Minimal Redis console (NoSQL power tool). Keeps one live connection per
//! named profile and runs arbitrary commands, formatting the reply for display.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct RedisManager {
    conns: Mutex<HashMap<String, redis::Connection>>,
}

impl RedisManager {
    /// Open (or replace) a connection and PING it.
    pub fn connect(&self, name: &str, url: &str) -> Result<String, String> {
        let client = redis::Client::open(url).map_err(|e| e.to_string())?;
        let mut con = client.get_connection().map_err(|e| e.to_string())?;
        let pong: String = redis::cmd("PING").query(&mut con).map_err(|e| e.to_string())?;
        self.conns.lock().unwrap().insert(name.to_string(), con);
        Ok(pong)
    }

    pub fn disconnect(&self, name: &str) {
        self.conns.lock().unwrap().remove(name);
    }

    pub fn connections(&self) -> Vec<String> {
        self.conns.lock().unwrap().keys().cloned().collect()
    }

    /// Run a command (already split into parts) and format the reply.
    pub fn command(&self, name: &str, parts: &[String]) -> Result<String, String> {
        if parts.is_empty() {
            return Err("empty command".into());
        }
        let mut guard = self.conns.lock().unwrap();
        let con = guard.get_mut(name).ok_or("not connected")?;
        let mut cmd = redis::cmd(&parts[0]);
        for a in &parts[1..] {
            cmd.arg(a);
        }
        let value: redis::Value = cmd.query(con).map_err(|e| e.to_string())?;
        Ok(format_value(&value, 0))
    }
}

fn format_value(v: &redis::Value, depth: usize) -> String {
    match v {
        redis::Value::Nil => "(nil)".to_string(),
        redis::Value::Int(i) => i.to_string(),
        redis::Value::SimpleString(s) => s.clone(),
        redis::Value::Okay => "OK".to_string(),
        redis::Value::BulkString(b) => String::from_utf8_lossy(b).to_string(),
        redis::Value::Array(items) | redis::Value::Set(items) => {
            let pad = "  ".repeat(depth);
            items
                .iter()
                .enumerate()
                .map(|(i, it)| format!("{pad}{}) {}", i + 1, format_value(it, depth + 1)))
                .collect::<Vec<_>>()
                .join("\n")
        }
        redis::Value::Map(pairs) => pairs
            .iter()
            .map(|(k, val)| format!("{} => {}", format_value(k, depth + 1), format_value(val, depth + 1)))
            .collect::<Vec<_>>()
            .join("\n"),
        redis::Value::Double(d) => d.to_string(),
        redis::Value::Boolean(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}
