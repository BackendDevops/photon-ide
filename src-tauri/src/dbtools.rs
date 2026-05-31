//! Database tools (Pillar 3, v1) — `docs/02-module-design.md` §database.
//!
//! A small connection manager + schema introspection + query runner built on
//! sqlx's `Any` driver, so MySQL, PostgreSQL, and SQLite share one code path.
//! Connection URLs:
//!   - MySQL:    `mysql://user:pass@host:3306/db`
//!   - Postgres: `postgres://user:pass@host:5432/db`
//!   - SQLite:   `sqlite:///absolute/path.db`  (or `sqlite::memory:`)
//!
//! NOTE: result-cell decoding is best-effort across engines (tries text, then
//! integer/float/bool). Verified against a live database, not the sandbox.

use serde::{Deserialize, Serialize};
use sqlx::{any::AnyPoolOptions, AnyPool, Column, Row};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Data sources (connection profiles) — the "Data Sources and Drivers" manager.
// Persisted to <project>/.photon/datasources.json. Passwords are only written
// when `save_password` is set (plaintext in v1; OS-keychain is the planned
// upgrade — see docs/09 §platform SecretStore).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSource {
    pub id: String,
    pub name: String,
    /// mysql | mariadb | postgres | sqlite
    pub driver: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub database: String,
    /// SQLite file path (when driver == "sqlite").
    #[serde(default)]
    pub sqlite_path: String,
    #[serde(default)]
    pub save_password: bool,
    #[serde(default)]
    pub password: Option<String>,
}

impl DataSource {
    pub fn default_port(&self) -> u16 {
        match self.driver.as_str() {
            "postgres" => 5432,
            _ => 3306,
        }
    }

    /// Build a sqlx connection URL, injecting `password` (overrides stored).
    pub fn url(&self, password: Option<&str>) -> String {
        let pw = password
            .map(|s| s.to_string())
            .or_else(|| self.password.clone())
            .unwrap_or_default();
        let port = if self.port == 0 { self.default_port() } else { self.port };
        match self.driver.as_str() {
            "sqlite" => {
                let p = if self.sqlite_path.starts_with('/') {
                    format!("sqlite://{}", self.sqlite_path)
                } else {
                    format!("sqlite://{}", self.sqlite_path)
                };
                format!("{p}?mode=rwc")
            }
            "postgres" => format!(
                "postgres://{}:{}@{}:{}/{}",
                self.user, pw, self.host, port, self.database
            ),
            // mysql + mariadb share the mysql wire protocol.
            _ => format!(
                "mysql://{}:{}@{}:{}/{}",
                self.user, pw, self.host, port, self.database
            ),
        }
    }

    /// A copy safe to send to the UI (password stripped).
    pub fn sanitized(&self) -> DataSource {
        let mut c = self.clone();
        c.password = None;
        c
    }
}

fn sources_path(project_root: &str) -> PathBuf {
    PathBuf::from(project_root).join(".photon").join("datasources.json")
}

pub fn load_sources(project_root: &str) -> Vec<DataSource> {
    let path = sources_path(project_root);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_sources(project_root: &str, sources: &[DataSource]) -> Result<(), String> {
    let path = sources_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Strip passwords we're not allowed to persist.
    let to_write: Vec<DataSource> = sources
        .iter()
        .map(|s| {
            let mut c = s.clone();
            if !c.save_password {
                c.password = None;
            }
            c
        })
        .collect();
    let json = serde_json::to_string_pretty(&to_write).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Upsert a source by id and persist. Returns the sanitized list.
pub fn save_source(project_root: &str, src: DataSource) -> Result<Vec<DataSource>, String> {
    let mut all = load_sources(project_root);
    match all.iter_mut().find(|s| s.id == src.id) {
        Some(existing) => *existing = src,
        None => all.push(src),
    }
    save_sources(project_root, &all)?;
    Ok(all.iter().map(DataSource::sanitized).collect())
}

pub fn delete_source(project_root: &str, id: &str) -> Result<Vec<DataSource>, String> {
    let mut all = load_sources(project_root);
    all.retain(|s| s.id != id);
    save_sources(project_root, &all)?;
    Ok(all.iter().map(DataSource::sanitized).collect())
}

/// Test a connection without keeping it open.
pub async fn test_url(url: &str) -> Result<String, String> {
    let pool = AnyPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .map_err(|e| e.to_string())?;
    // A trivial round-trip.
    sqlx::query("SELECT 1")
        .fetch_optional(&pool)
        .await
        .map_err(|e| e.to_string())?;
    pool.close().await;
    Ok("Connection successful".into())
}

struct Conn {
    pool: AnyPool,
    engine: String,
}

#[derive(Default)]
pub struct DbManager {
    conns: Mutex<HashMap<String, Conn>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTable {
    pub name: String,
    pub columns: Vec<DbColumn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSchema {
    pub engine: String,
    pub tables: Vec<DbTable>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: usize,
    pub affected: Option<u64>,
}

fn engine_of(url: &str) -> &'static str {
    if url.starts_with("postgres") {
        "postgres"
    } else if url.starts_with("mysql") {
        "mysql"
    } else if url.starts_with("sqlite") {
        "sqlite"
    } else {
        "unknown"
    }
}

impl DbManager {
    /// Open a pooled connection and remember it under `name`. Returns the engine.
    pub async fn connect(&self, name: &str, url: &str) -> Result<String, String> {
        let engine = engine_of(url).to_string();
        let pool = AnyPoolOptions::new()
            .max_connections(4)
            .connect(url)
            .await
            .map_err(|e| e.to_string())?;
        self.conns.lock().await.insert(
            name.to_string(),
            Conn {
                pool,
                engine: engine.clone(),
            },
        );
        Ok(engine)
    }

    pub async fn disconnect(&self, name: &str) {
        if let Some(c) = self.conns.lock().await.remove(name) {
            c.pool.close().await;
        }
    }

    pub async fn connections(&self) -> Vec<String> {
        self.conns.lock().await.keys().cloned().collect()
    }

    async fn get(&self, name: &str) -> Result<(AnyPool, String), String> {
        let guard = self.conns.lock().await;
        let c = guard
            .get(name)
            .ok_or_else(|| format!("no connection named '{name}'"))?;
        Ok((c.pool.clone(), c.engine.clone()))
    }

    /// Introspect tables + columns. Engine-specific queries, unified output.
    pub async fn schema(&self, name: &str) -> Result<DbSchema, String> {
        let (pool, engine) = self.get(name).await?;

        let table_sql = match engine.as_str() {
            "sqlite" => "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            "postgres" => "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name",
            _ => "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE() ORDER BY table_name",
        };
        let rows = sqlx::query(table_sql)
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?;

        let mut tables = Vec::new();
        for r in rows {
            let tname: String = r.try_get(0).unwrap_or_default();
            if tname.is_empty() {
                continue;
            }
            let columns = self.columns(&pool, &engine, &tname).await.unwrap_or_default();
            tables.push(DbTable { name: tname, columns });
        }
        Ok(DbSchema { engine, tables })
    }

    async fn columns(
        &self,
        pool: &AnyPool,
        engine: &str,
        table: &str,
    ) -> Result<Vec<DbColumn>, String> {
        let mut out = Vec::new();
        if engine == "sqlite" {
            let q = format!("PRAGMA table_info('{}')", table.replace('\'', "''"));
            let rows = sqlx::query(&q).fetch_all(pool).await.map_err(|e| e.to_string())?;
            for r in rows {
                let name: String = r.try_get("name").unwrap_or_default();
                let data_type: String = r.try_get("type").unwrap_or_default();
                let notnull: i64 = r.try_get("notnull").unwrap_or(0);
                out.push(DbColumn {
                    name,
                    data_type,
                    nullable: notnull == 0,
                });
            }
        } else {
            let schema_filter = if engine == "postgres" {
                "table_schema = 'public'"
            } else {
                "table_schema = DATABASE()"
            };
            // Table name comes from our own introspection query (trusted), so we
            // inline it after escaping quotes. This avoids sqlx `Any` placeholder
            // differences ('?' vs '$1') across MySQL and PostgreSQL.
            let safe = table.replace('\'', "''");
            let q = format!(
                "SELECT column_name, data_type, is_nullable FROM information_schema.columns
                 WHERE {schema_filter} AND table_name = '{safe}' ORDER BY ordinal_position"
            );
            let rows = sqlx::query(&q)
                .fetch_all(pool)
                .await
                .map_err(|e| e.to_string())?;
            for r in rows {
                let name: String = r.try_get("column_name").unwrap_or_default();
                let data_type: String = r.try_get("data_type").unwrap_or_default();
                let is_nullable: String = r.try_get("is_nullable").unwrap_or_default();
                out.push(DbColumn {
                    name,
                    data_type,
                    nullable: is_nullable.eq_ignore_ascii_case("yes"),
                });
            }
        }
        Ok(out)
    }

    /// Run arbitrary SQL. SELECT-like returns rows; others return affected count.
    pub async fn query(&self, name: &str, sql: &str) -> Result<QueryResult, String> {
        let (pool, _engine) = self.get(name).await?;
        let trimmed = sql.trim_start().to_lowercase();
        let is_read = trimmed.starts_with("select")
            || trimmed.starts_with("with")
            || trimmed.starts_with("pragma")
            || trimmed.starts_with("show")
            || trimmed.starts_with("explain");

        if is_read {
            let rows = sqlx::query(sql).fetch_all(&pool).await.map_err(|e| e.to_string())?;
            let mut columns: Vec<String> = Vec::new();
            if let Some(first) = rows.first() {
                columns = first.columns().iter().map(|c| c.name().to_string()).collect();
            }
            let mut out_rows = Vec::with_capacity(rows.len());
            for r in &rows {
                let ncols = r.columns().len();
                let mut cells = Vec::with_capacity(ncols);
                for i in 0..ncols {
                    cells.push(cell_to_string(r, i));
                }
                out_rows.push(cells);
            }
            Ok(QueryResult {
                row_count: out_rows.len(),
                columns,
                rows: out_rows,
                affected: None,
            })
        } else {
            let res = sqlx::query(sql).execute(&pool).await.map_err(|e| e.to_string())?;
            Ok(QueryResult {
                columns: vec!["result".into()],
                rows: vec![vec![format!("{} row(s) affected", res.rows_affected())]],
                row_count: 0,
                affected: Some(res.rows_affected()),
            })
        }
    }
}

impl DbManager {
    /// Inline edit: UPDATE one cell, identified by a primary-key column/value.
    /// Identifiers come from schema introspection; values are bound (safe).
    pub async fn update_cell(
        &self,
        name: &str,
        table: &str,
        column: &str,
        value: &str,
        pk_column: &str,
        pk_value: &str,
    ) -> Result<u64, String> {
        let (pool, engine) = self.get(name).await?;
        let (p1, p2) = if engine == "postgres" { ("$1", "$2") } else { ("?", "?") };
        let sql = format!("UPDATE {table} SET {column} = {p1} WHERE {pk_column} = {p2}");
        let res = sqlx::query(&sql)
            .bind(value)
            .bind(pk_value)
            .execute(&pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(res.rows_affected())
    }
}

/// Best-effort cell formatter: try common types, fall back to NULL/?.
fn cell_to_string(row: &sqlx::any::AnyRow, idx: usize) -> String {
    use sqlx::ValueRef;
    if let Ok(raw) = row.try_get_raw(idx) {
        if raw.is_null() {
            return "NULL".to_string();
        }
    }
    if let Ok(v) = row.try_get::<String, _>(idx) {
        return v;
    }
    if let Ok(v) = row.try_get::<i64, _>(idx) {
        return v.to_string();
    }
    if let Ok(v) = row.try_get::<f64, _>(idx) {
        return v.to_string();
    }
    if let Ok(v) = row.try_get::<bool, _>(idx) {
        return v.to_string();
    }
    "?".to_string()
}
