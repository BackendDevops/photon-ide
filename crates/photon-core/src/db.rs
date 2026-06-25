//! The index store. A pragmatic subset of `docs/03-database-schema.md`,
//! backed by SQLite. Holds files, symbols, and Laravel routes, and answers
//! the lookups that power navigation and Search Everywhere.

use crate::types::{FileEntry, RefKind, Reference, Route, Symbol, SymbolKind};
use rusqlite::{params, Connection};

pub struct Index {
    pub conn: Connection,
}

impl Index {
    /// Open (or create) the index. Pass ":memory:" for an ephemeral index.
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let idx = Index { conn };
        idx.migrate()?;
        Ok(idx)
    }

    fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS files (
                id        INTEGER PRIMARY KEY,
                path      TEXT NOT NULL UNIQUE,
                lang      TEXT NOT NULL,
                size      INTEGER NOT NULL,
                is_vendor INTEGER NOT NULL DEFAULT 0,
                mtime     INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS symbols (
                id          INTEGER PRIMARY KEY,
                name        TEXT NOT NULL,
                fqn         TEXT,
                kind        TEXT NOT NULL,
                file        TEXT NOT NULL,
                container   TEXT,
                line        INTEGER NOT NULL,
                name_offset INTEGER NOT NULL,
                range_start INTEGER NOT NULL DEFAULT 0,
                range_end   INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_fqn  ON symbols(fqn);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
            CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind);

            CREATE TABLE IF NOT EXISTS routes (
                id     INTEGER PRIMARY KEY,
                method TEXT NOT NULL,
                uri    TEXT NOT NULL,
                name   TEXT,
                action TEXT,
                file   TEXT NOT NULL,
                line   INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_routes_name ON routes(name);

            CREATE TABLE IF NOT EXISTS refs (
                id     INTEGER PRIMARY KEY,
                name   TEXT NOT NULL,
                kind   TEXT NOT NULL,
                file   TEXT NOT NULL,
                line   INTEGER NOT NULL,
                col    INTEGER NOT NULL,
                start  INTEGER NOT NULL,
                end    INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_refs_name ON refs(name);
            CREATE INDEX IF NOT EXISTS idx_refs_file ON refs(file);

            CREATE TABLE IF NOT EXISTS models (
                symbol_id INTEGER,
                name      TEXT NOT NULL,
                fqn       TEXT,
                tbl       TEXT,
                file      TEXT NOT NULL,
                line      INTEGER NOT NULL,
                fillable  TEXT,
                PRIMARY KEY (name, file)
            );

            CREATE TABLE IF NOT EXISTS relations (
                model    TEXT NOT NULL,
                method   TEXT NOT NULL,
                rel_type TEXT NOT NULL,
                related  TEXT,
                file     TEXT NOT NULL,
                line     INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_relations_model ON relations(model);

            CREATE TABLE IF NOT EXISTS config_keys (
                key  TEXT NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL,
                PRIMARY KEY (key, file)
            );

            CREATE TABLE IF NOT EXISTS translations (
                key    TEXT NOT NULL,
                locale TEXT NOT NULL,
                file   TEXT NOT NULL,
                line   INTEGER NOT NULL,
                PRIMARY KEY (key, locale, file)
            );
            CREATE INDEX IF NOT EXISTS idx_trans_key ON translations(key);

            CREATE TABLE IF NOT EXISTS bindings (
                abstract_name TEXT NOT NULL, concrete TEXT, kind TEXT NOT NULL,
                file TEXT NOT NULL, line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bindings_abstract ON bindings(abstract_name);

            CREATE TABLE IF NOT EXISTS events (
                event TEXT NOT NULL, listener TEXT NOT NULL,
                file TEXT NOT NULL, line INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_event ON events(event);

            CREATE TABLE IF NOT EXISTS jobs (
                name TEXT NOT NULL, fqn TEXT, queued INTEGER NOT NULL,
                file TEXT NOT NULL, line INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                name TEXT NOT NULL, kind TEXT NOT NULL, related TEXT,
                file TEXT NOT NULL, line INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS member_types (
                container TEXT NOT NULL,
                member    TEXT NOT NULL,
                kind      TEXT NOT NULL,
                ty        TEXT NOT NULL,
                file      TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_membertypes ON member_types(container, member);

            CREATE TABLE IF NOT EXISTS type_relations (
                src  TEXT NOT NULL,
                dst  TEXT NOT NULL,
                rel  TEXT NOT NULL,
                file TEXT NOT NULL,
                line INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_typerel_dst ON type_relations(dst, rel);
            CREATE INDEX IF NOT EXISTS idx_typerel_src ON type_relations(src);

            CREATE TABLE IF NOT EXISTS mig_columns (
                tbl      TEXT NOT NULL,
                col      TEXT NOT NULL,
                col_type TEXT NOT NULL DEFAULT 'string',
                file     TEXT NOT NULL,
                PRIMARY KEY (tbl, col, file)
            );
            CREATE INDEX IF NOT EXISTS idx_migcol_tbl ON mig_columns(tbl);

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT
            );
            "#,
        )?;
        // Defensive migration for index files created before `mtime` existed.
        let _ = self
            .conn
            .execute("ALTER TABLE files ADD COLUMN mtime INTEGER NOT NULL DEFAULT 0", []);
        Ok(())
    }

    /// Clear all rows (used for a full re-index in the MVP).
    /// Stub rows (file = "__stub__") in member_types / type_relations are
    /// intentionally preserved so framework stubs survive a re-index cycle.
    pub fn clear(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(
            "DELETE FROM files; DELETE FROM symbols; DELETE FROM routes;
             DELETE FROM refs; DELETE FROM models; DELETE FROM relations;
             DELETE FROM config_keys; DELETE FROM translations;
             DELETE FROM bindings; DELETE FROM events; DELETE FROM jobs;
             DELETE FROM artifacts; DELETE FROM mig_columns;
             DELETE FROM type_relations WHERE file != '__stub__';
             DELETE FROM member_types   WHERE file != '__stub__';",
        )?;
        Ok(())
    }

    pub fn replace_laravel_facts_for_file(
        &mut self,
        file: &str,
        bindings: &[crate::types::Binding],
        events: &[crate::types::EventListener],
        jobs: &[crate::types::JobInfo],
        artifacts: &[crate::types::ArtifactInfo],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        for t in ["bindings", "events", "jobs", "artifacts"] {
            tx.execute(&format!("DELETE FROM {t} WHERE file = ?1"), params![file])?;
        }
        {
            let mut s = tx.prepare(
                "INSERT INTO bindings(abstract_name, concrete, kind, file, line) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for b in bindings {
                s.execute(params![b.abstract_name, b.concrete, b.kind, b.file, b.line as i64])?;
            }
            let mut e = tx.prepare(
                "INSERT INTO events(event, listener, file, line) VALUES (?1,?2,?3,?4)",
            )?;
            for ev in events {
                e.execute(params![ev.event, ev.listener, ev.file, ev.line as i64])?;
            }
            let mut j = tx.prepare(
                "INSERT INTO jobs(name, fqn, queued, file, line) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for job in jobs {
                j.execute(params![job.name, job.fqn, job.queued as i64, job.file, job.line as i64])?;
            }
            let mut a = tx.prepare(
                "INSERT INTO artifacts(name, kind, related, file, line) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for art in artifacts {
                a.execute(params![art.name, art.kind, art.related, art.file, art.line as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn bindings(&self) -> anyhow::Result<Vec<crate::types::Binding>> {
        let mut stmt = self.conn.prepare(
            "SELECT abstract_name, concrete, kind, file, line FROM bindings ORDER BY abstract_name",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::types::Binding {
                abstract_name: r.get(0)?,
                concrete: r.get(1)?,
                kind: r.get(2)?,
                file: r.get(3)?,
                line: r.get::<_, i64>(4)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn events(&self) -> anyhow::Result<Vec<crate::types::EventListener>> {
        let mut stmt = self
            .conn
            .prepare("SELECT event, listener, file, line FROM events ORDER BY event")?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::types::EventListener {
                event: r.get(0)?,
                listener: r.get(1)?,
                file: r.get(2)?,
                line: r.get::<_, i64>(3)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn jobs(&self) -> anyhow::Result<Vec<crate::types::JobInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, fqn, queued, file, line FROM jobs ORDER BY name")?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::types::JobInfo {
                name: r.get(0)?,
                fqn: r.get(1)?,
                queued: r.get::<_, i64>(2)? != 0,
                file: r.get(3)?,
                line: r.get::<_, i64>(4)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn artifacts(&self) -> anyhow::Result<Vec<crate::types::ArtifactInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, kind, related, file, line FROM artifacts ORDER BY kind, name")?;
        let rows = stmt.query_map([], |r| {
            Ok(crate::types::ArtifactInfo {
                name: r.get(0)?,
                kind: r.get(1)?,
                related: r.get(2)?,
                file: r.get(3)?,
                line: r.get::<_, i64>(4)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn insert_files(&mut self, files: &[FileEntry]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO files(path, lang, size, is_vendor, mtime) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for f in files {
                stmt.execute(params![
                    f.path,
                    f.lang,
                    f.size as i64,
                    f.is_vendor as i64,
                    f.mtime as i64
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_member_types_for_file(
        &mut self,
        file: &str,
        types: &[(String, String, String, String)],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM member_types WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO member_types(container, member, kind, ty, file) VALUES (?1,?2,?3,?4,?5)",
            )?;
            for (c, m, k, t) in types {
                stmt.execute(params![c, m, k, t, file])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Declared type of a member (method return / property), if known.
    pub fn member_type(&self, container: &str, member: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT ty FROM member_types WHERE container = ?1 AND member = ?2
             AND kind IN ('method','property') LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![container, member], |r| r.get::<_, String>(0))?;
        Ok(rows.next().and_then(Result::ok))
    }

    /// Generic element type of a collection-valued member, if documented
    /// (`@property Collection<Order> $orders` → `Order`).
    pub fn member_element_type(&self, container: &str, member: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT ty FROM member_types WHERE container = ?1 AND member = ?2
             AND kind IN ('method_item','property_item') LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![container, member], |r| r.get::<_, String>(0))?;
        Ok(rows.next().and_then(Result::ok))
    }

    pub fn replace_type_relations_for_file(
        &mut self,
        file: &str,
        rels: &[(String, String, String)],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM type_relations WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO type_relations(src, dst, rel, file) VALUES (?1,?2,?3,?4)",
            )?;
            for (s, d, r) in rels {
                stmt.execute(params![s, d, r, file])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Direct supertypes of `name` (parent class + used traits) for member-set
    /// walking. Returns the `dst` names of extends/uses relations.
    pub fn supertypes(&self, name: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT dst FROM type_relations WHERE src = ?1 AND rel IN ('extends','uses','mixin')",
        )?;
        let rows = stmt.query_map(params![name], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Types that extend or implement `name` (Go-to-Implementation / subclasses).
    pub fn implementations_of(&self, name: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT src FROM type_relations
             WHERE dst = ?1 AND rel IN ('implements','extends') ORDER BY src",
        )?;
        let rows = stmt.query_map(params![name], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Insert many symbols in a single transaction (no per-file delete).
    /// For one-shot bulk indexing (vendor) — far faster than per-file replace.
    pub fn insert_symbols_bulk(&mut self, symbols: &[Symbol]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbols(name, fqn, kind, file, container, line, name_offset, range_start, range_end)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )?;
            for s in symbols {
                stmt.execute(params![
                    s.name,
                    s.fqn,
                    s.kind.as_str(),
                    s.file,
                    s.container,
                    s.line as i64,
                    s.name_offset as i64,
                    s.range_start as i64,
                    s.range_end as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Replace all symbols for a file (incremental per-file update).
    pub fn replace_symbols_for_file(&mut self, file: &str, symbols: &[Symbol]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM symbols WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbols(name, fqn, kind, file, container, line, name_offset, range_start, range_end)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )?;
            for s in symbols {
                stmt.execute(params![
                    s.name,
                    s.fqn,
                    s.kind.as_str(),
                    s.file,
                    s.container,
                    s.line as i64,
                    s.name_offset as i64,
                    s.range_start as i64,
                    s.range_end as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_routes_for_file(&mut self, file: &str, routes: &[Route]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM routes WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO routes(method, uri, name, action, file, line)
                 VALUES (?1,?2,?3,?4,?5,?6)",
            )?;
            for r in routes {
                stmt.execute(params![r.method, r.uri, r.name, r.action, r.file, r.line as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_refs_for_file(&mut self, file: &str, refs: &[Reference]) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM refs WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO refs(name, kind, file, line, col, start, end)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )?;
            for r in refs {
                stmt.execute(params![
                    r.name,
                    r.kind.as_str(),
                    r.file,
                    r.line as i64,
                    r.column as i64,
                    r.start as i64,
                    r.end as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// All references to a given short name (Find Usages).
    pub fn references_to(&self, name: &str) -> anyhow::Result<Vec<Reference>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, kind, file, line, col, start, end FROM refs
             WHERE name = ?1 ORDER BY file, line",
        )?;
        let rows = stmt.query_map(params![name], |r| {
            Ok(Reference {
                name: r.get(0)?,
                kind: RefKind::from_str(&r.get::<_, String>(1)?),
                file: r.get(2)?,
                line: r.get::<_, i64>(3)? as u32,
                column: r.get::<_, i64>(4)? as u32,
                start: r.get::<_, i64>(5)? as u32,
                end: r.get::<_, i64>(6)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn replace_models_for_file(
        &mut self,
        file: &str,
        models: &[crate::types::ModelInfo],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM models WHERE file = ?1", params![file])?;
        tx.execute("DELETE FROM relations WHERE file = ?1", params![file])?;
        {
            let mut ms = tx.prepare(
                "INSERT OR REPLACE INTO models(symbol_id, name, fqn, tbl, file, line, fillable)
                 VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut rs = tx.prepare(
                "INSERT INTO relations(model, method, rel_type, related, file, line)
                 VALUES (?1,?2,?3,?4,?5,?6)",
            )?;
            for m in models {
                ms.execute(params![
                    m.name,
                    m.fqn,
                    m.table,
                    m.file,
                    m.line as i64,
                    serde_json::to_string(&m.fillable).unwrap_or_default(),
                ])?;
                for rel in &m.relations {
                    rs.execute(params![
                        m.name,
                        rel.method,
                        rel.rel_type,
                        rel.related,
                        m.file,
                        rel.line as i64,
                    ])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Just the model short names (cheap — for the type-chain resolver).
    pub fn model_names(&self) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM models")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn models(&self) -> anyhow::Result<Vec<crate::types::ModelInfo>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, fqn, tbl, file, line, fillable FROM models ORDER BY name")?;
        let base: Vec<(String, Option<String>, Option<String>, String, u32, String)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? as u32,
                    r.get::<_, String>(5)?,
                ))
            })?
            .filter_map(Result::ok)
            .collect();

        let mut out = Vec::new();
        for (name, fqn, table, file, line, fillable_json) in base {
            let relations = self.relations_for(&name)?;
            out.push(crate::types::ModelInfo {
                name,
                fqn,
                table,
                file,
                line,
                fillable: serde_json::from_str(&fillable_json).unwrap_or_default(),
                relations,
            });
        }
        Ok(out)
    }

    fn relations_for(&self, model: &str) -> anyhow::Result<Vec<crate::types::RelationInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT method, rel_type, related, line FROM relations WHERE model = ?1 ORDER BY line",
        )?;
        let rows = stmt.query_map(params![model], |r| {
            Ok(crate::types::RelationInfo {
                method: r.get(0)?,
                rel_type: r.get(1)?,
                related: r.get(2)?,
                line: r.get::<_, i64>(3)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn replace_config_keys_for_file(
        &mut self,
        file: &str,
        keys: &[crate::types::KeyEntry],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM config_keys WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO config_keys(key, file, line) VALUES (?1,?2,?3)",
            )?;
            for k in keys {
                stmt.execute(params![k.key, k.file, k.line as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn config_key(&self, key: &str) -> anyhow::Result<Option<crate::types::KeyEntry>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, file, line FROM config_keys WHERE key = ?1 LIMIT 1")?;
        let mut rows = stmt.query_map(params![key], |r| {
            Ok(crate::types::KeyEntry {
                key: r.get(0)?,
                locale: String::new(),
                file: r.get(1)?,
                line: r.get::<_, i64>(2)? as u32,
            })
        })?;
        Ok(rows.next().and_then(Result::ok))
    }

    /// True if any config key exists under a top-level namespace (config file),
    /// e.g. namespace "pages" matches "pages" or "pages.foo.bar". Used to avoid
    /// false "unknown config key" warnings for dynamic deep keys.
    pub fn config_namespace_known(&self, seg: &str) -> anyhow::Result<bool> {
        let prefix = format!("{}.%", seg);
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM config_keys WHERE key = ?1 OR key LIKE ?2",
            params![seg, prefix],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn translation_namespace_known(&self, seg: &str) -> anyhow::Result<bool> {
        let prefix = format!("{}.%", seg);
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM translations WHERE key = ?1 OR key LIKE ?2",
            params![seg, prefix],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn config_key_candidates(&self, like: &str, limit: usize) -> anyhow::Result<Vec<crate::types::KeyEntry>> {
        let pattern = format!("%{}%", like);
        let mut stmt = self.conn.prepare(
            "SELECT key, file, line FROM config_keys WHERE key LIKE ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |r| {
            Ok(crate::types::KeyEntry {
                key: r.get(0)?,
                locale: String::new(),
                file: r.get(1)?,
                line: r.get::<_, i64>(2)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn replace_translations_for_file(
        &mut self,
        file: &str,
        keys: &[crate::types::KeyEntry],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM translations WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO translations(key, locale, file, line) VALUES (?1,?2,?3,?4)",
            )?;
            for k in keys {
                stmt.execute(params![k.key, k.locale, k.file, k.line as i64])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn translation(&self, key: &str) -> anyhow::Result<Vec<crate::types::KeyEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT key, locale, file, line FROM translations WHERE key = ?1 ORDER BY locale",
        )?;
        let rows = stmt.query_map(params![key], |r| {
            Ok(crate::types::KeyEntry {
                key: r.get(0)?,
                locale: r.get(1)?,
                file: r.get(2)?,
                line: r.get::<_, i64>(3)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Translation keys present in some locales but missing in others.
    pub fn missing_translations(&self) -> anyhow::Result<Vec<crate::types::MissingTranslation>> {
        // All locales that exist in the project.
        let mut locstmt = self.conn.prepare("SELECT DISTINCT locale FROM translations")?;
        let locales: Vec<String> = locstmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();
        if locales.len() < 2 {
            return Ok(Vec::new());
        }

        let mut keystmt = self.conn.prepare("SELECT DISTINCT key FROM translations")?;
        let keys: Vec<String> = keystmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();

        let mut out = Vec::new();
        for key in keys {
            let mut present = Vec::new();
            let mut stmt = self
                .conn
                .prepare("SELECT DISTINCT locale FROM translations WHERE key = ?1")?;
            let got: Vec<String> = stmt
                .query_map(params![key], |r| r.get::<_, String>(0))?
                .filter_map(Result::ok)
                .collect();
            for l in &locales {
                if got.contains(l) {
                    present.push(l.clone());
                }
            }
            let missing: Vec<String> =
                locales.iter().filter(|l| !present.contains(l)).cloned().collect();
            if !missing.is_empty() {
                out.push(crate::types::MissingTranslation {
                    key,
                    present_in: present,
                    missing_in: missing,
                });
            }
        }
        Ok(out)
    }

    pub fn replace_mig_columns_for_file(
        &mut self,
        file: &str,
        cols: &[(String, String, String)],
    ) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM mig_columns WHERE file = ?1", params![file])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO mig_columns(tbl, col, col_type, file) VALUES (?1,?2,?3,?4)",
            )?;
            for (tbl, col, ty) in cols {
                stmt.execute(params![tbl, col, ty, file])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Real columns for a table, gathered from migrations.
    /// Every table and its columns (from migrations) — schema-aware SQL
    /// completion inside PHP string literals.
    pub fn tables_with_columns(&self) -> anyhow::Result<Vec<(String, Vec<String>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT tbl, col FROM mig_columns ORDER BY tbl, col")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for (tbl, col) in rows.filter_map(Result::ok) {
            map.entry(tbl).or_default().push(col);
        }
        Ok(map.into_iter().collect())
    }

    pub fn columns_for_table(&self, table: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT col FROM mig_columns WHERE tbl = ?1 ORDER BY col")?;
        let rows = stmt.query_map(params![table], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Columns with PHP types (for model PHPDoc generation).
    pub fn columns_with_types(&self, table: &str) -> anyhow::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT col, col_type FROM mig_columns WHERE tbl = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![table], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Remove all rows belonging to one workspace root (paths `"<label>/…"`).
    pub fn clear_root(&self, label: &str) -> anyhow::Result<()> {
        let pat = format!("{}/%", label);
        self.conn
            .execute("DELETE FROM files WHERE path LIKE ?1", params![pat])?;
        for t in [
            "symbols", "refs", "routes", "models", "relations", "config_keys",
            "translations", "bindings", "events", "jobs", "artifacts", "mig_columns",
            "type_relations", "member_types",
        ] {
            self.conn
                .execute(&format!("DELETE FROM {t} WHERE file LIKE ?1"), params![pat])?;
        }
        Ok(())
    }

    pub fn count_php_files(&self) -> anyhow::Result<u32> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE lang IN ('php','blade')",
            [],
            |r| r.get(0),
        )?;
        Ok(n as u32)
    }

    pub fn count(&self, table: &str) -> anyhow::Result<u32> {
        let n: i64 = self
            .conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
        Ok(n as u32)
    }

    /// Outline / symbols for a single file, ordered by line.
    pub fn symbols_in_file(&self, file: &str) -> anyhow::Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, fqn, kind, file, container, line, name_offset, range_start, range_end
             FROM symbols WHERE file = ?1 ORDER BY line",
        )?;
        let rows = stmt.query_map(params![file], row_to_symbol)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// For every class/interface/trait/enum/function/method declared in `file`,
    /// return `(name, line, ref_count)` where `ref_count` is how many references
    /// to that name exist across the entire index. Used for code-lens display.
    ///
    /// A single JOIN is far cheaper than N individual `references_to` calls.
    pub fn reference_counts_for_file(
        &self,
        file: &str,
    ) -> anyhow::Result<Vec<(String, u32, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.name, s.line, COUNT(r.rowid) AS cnt
             FROM symbols s
             LEFT JOIN refs r ON r.name = s.name
             WHERE s.file = ?1
               AND s.kind IN ('class','interface','trait','enum','function','method')
             GROUP BY s.name, s.line
             HAVING cnt > 0
             ORDER BY s.line",
        )?;
        let rows = stmt.query_map(params![file], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? as u32,
                r.get::<_, i64>(2)? as u64,
            ))
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// All routes ordered by uri.
    pub fn routes(&self) -> anyhow::Result<Vec<Route>> {
        let mut stmt = self
            .conn
            .prepare("SELECT method, uri, name, action, file, line FROM routes ORDER BY uri")?;
        let rows = stmt.query_map([], |r| {
            Ok(Route {
                method: r.get(0)?,
                uri: r.get(1)?,
                name: r.get(2)?,
                action: r.get(3)?,
                file: r.get(4)?,
                line: r.get::<_, i64>(5)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Members (methods/properties/consts) declared in a container class.
    pub fn members_of(&self, container: &str) -> anyhow::Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, fqn, kind, file, container, line, name_offset, range_start, range_end
             FROM symbols WHERE container = ?1 ORDER BY kind, name",
        )?;
        let rows = stmt.query_map(params![container], row_to_symbol)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Find a symbol by exact name (go-to-symbol).
    pub fn find_symbol(&self, name: &str) -> anyhow::Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, fqn, kind, file, container, line, name_offset, range_start, range_end
             FROM symbols WHERE name = ?1 LIMIT 50",
        )?;
        let rows = stmt.query_map(params![name], row_to_symbol)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Candidate symbols whose name contains the query (case-insensitive),
    /// for the fuzzy ranker in `search.rs`.
    pub fn symbol_candidates(&self, like: &str, limit: usize) -> anyhow::Result<Vec<Symbol>> {
        let pattern = format!("%{}%", like);
        let mut stmt = self.conn.prepare(
            "SELECT name, fqn, kind, file, container, line, name_offset, range_start, range_end
             FROM symbols WHERE name LIKE ?1 COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], row_to_symbol)?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn file_candidates(&self, like: &str, limit: usize) -> anyhow::Result<Vec<FileEntry>> {
        let pattern = format!("%{}%", like);
        let mut stmt = self.conn.prepare(
            "SELECT path, lang, size, is_vendor, mtime FROM files
             WHERE path LIKE ?1 COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |r| {
            Ok(FileEntry {
                path: r.get(0)?,
                lang: r.get(1)?,
                size: r.get::<_, i64>(2)? as u64,
                is_vendor: r.get::<_, i64>(3)? != 0,
                mtime: r.get::<_, i64>(4)? as u64,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn route_candidates(&self, like: &str, limit: usize) -> anyhow::Result<Vec<Route>> {
        let pattern = format!("%{}%", like);
        let mut stmt = self.conn.prepare(
            "SELECT method, uri, name, action, file, line FROM routes
             WHERE uri LIKE ?1 OR name LIKE ?1 COLLATE NOCASE LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![pattern, limit as i64], |r| {
            Ok(Route {
                method: r.get(0)?,
                uri: r.get(1)?,
                name: r.get(2)?,
                action: r.get(3)?,
                file: r.get(4)?,
                line: r.get::<_, i64>(5)? as u32,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |r| r.get::<_, String>(0))?;
        Ok(rows.next().and_then(Result::ok))
    }

    /// Gate a persistent index against schema drift. If the stored schema
    /// version differs from `current` (or an older, unversioned index has data),
    /// wipe everything so the caller re-indexes from scratch. Always records the
    /// current version. Safe to call on a fresh (empty) database.
    pub fn ensure_schema_version(&self, current: i64) -> anyhow::Result<()> {
        let stored: Option<i64> = self
            .get_meta("schema_version")?
            .and_then(|v| v.parse::<i64>().ok());
        match stored {
            Some(v) if v == current => {}
            Some(_) => self.clear()?,
            None => {
                if self.count("files").unwrap_or(0) > 0 {
                    self.clear()?; // unversioned index of unknown shape → reset
                }
            }
        }
        self.set_meta("schema_version", &current.to_string())?;
        Ok(())
    }

    /// `path -> mtime` for every file under a root label (warm-start compare).
    pub fn file_mtimes_with_prefix(
        &self,
        label: &str,
    ) -> anyhow::Result<std::collections::HashMap<String, u64>> {
        let pat = format!("{}/%", label);
        let mut stmt = self
            .conn
            .prepare("SELECT path, mtime FROM files WHERE path LIKE ?1")?;
        let rows = stmt.query_map(params![pat], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Insert/update a single file row (used during reconcile).
    pub fn upsert_file(&self, f: &FileEntry) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO files(path, lang, size, is_vendor, mtime) VALUES (?1,?2,?3,?4,?5)",
            params![f.path, f.lang, f.size as i64, f.is_vendor as i64, f.mtime as i64],
        )?;
        Ok(())
    }

    /// Drop a single file and every fact derived from it (deleted-on-disk file).
    pub fn delete_file_rows(&self, path: &str) -> anyhow::Result<()> {
        self.conn.execute("DELETE FROM files WHERE path = ?1", params![path])?;
        for t in [
            "symbols", "refs", "routes", "models", "relations", "config_keys",
            "translations", "bindings", "events", "jobs", "artifacts", "mig_columns",
            "type_relations", "member_types",
        ] {
            self.conn
                .execute(&format!("DELETE FROM {t} WHERE file = ?1"), params![path])?;
        }
        Ok(())
    }

    /// Remove all symbols belonging to vendor files (keeps `index_vendor`
    /// idempotent across warm starts so symbols never duplicate).
    pub fn clear_vendor_symbols(&self) -> anyhow::Result<()> {
        self.conn.execute(
            "DELETE FROM symbols WHERE file IN (SELECT path FROM files WHERE is_vendor = 1)",
            [],
        )?;
        Ok(())
    }

    /// Load pre-built framework stubs (JSON bytes) into member_types and
    /// type_relations with file = "__stub__". Always replaces existing stubs
    /// so a new app version picks up updated method lists automatically.
    pub fn load_stubs(&mut self, json: &str) -> anyhow::Result<()> {
        let stubs: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| anyhow::anyhow!("stub JSON parse error: {e}"))?;
        let obj = stubs
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("stubs root must be a JSON object"))?;
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM member_types   WHERE file = '__stub__'", [])?;
        tx.execute("DELETE FROM type_relations WHERE file = '__stub__'", [])?;
        {
            let mut mt = tx.prepare(
                "INSERT INTO member_types(container, member, kind, ty, file)
                 VALUES (?1, ?2, ?3, ?4, '__stub__')",
            )?;
            let mut tr = tx.prepare(
                "INSERT INTO type_relations(src, dst, rel, file, line)
                 VALUES (?1, ?2, 'extends', '__stub__', 0)",
            )?;
            let mut tr_mixin = tx.prepare(
                "INSERT INTO type_relations(src, dst, rel, file, line)
                 VALUES (?1, ?2, 'mixin', '__stub__', 0)",
            )?;
            for (fqn, def) in obj {
                let short = fqn.rsplit('\\').next().unwrap_or(fqn.as_str());
                if let Some(methods) = def.get("methods").and_then(|m| m.as_object()) {
                    for (mname, sig) in methods {
                        let ret = sig.get("return").and_then(|v| v.as_str()).unwrap_or("mixed");
                        mt.execute(params![fqn, mname, "method", ret])?;
                        if short != fqn.as_str() {
                            mt.execute(params![short, mname, "method", ret])?;
                        }
                    }
                }
                if let Some(parent_fqn) = def.get("extends").and_then(|v| v.as_str()) {
                    let parent_short = parent_fqn.rsplit('\\').next().unwrap_or(parent_fqn);
                    tr.execute(params![fqn, parent_fqn])?;
                    if short != fqn.as_str() {
                        tr.execute(params![short, parent_short])?;
                    }
                }
                if let Some(mixin_fqn) = def.get("mixin").and_then(|v| v.as_str()) {
                    let mixin_short = mixin_fqn.rsplit('\\').next().unwrap_or(mixin_fqn);
                    tr_mixin.execute(params![fqn, mixin_fqn])?;
                    if short != fqn.as_str() {
                        tr_mixin.execute(params![short, mixin_short])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Resolve an abstract class / interface name to its concrete binding from
    /// a service provider registration (`bind`, `singleton`, etc.). Returns the
    /// `concrete` column, or `None` if not found / bound to a Closure.
    pub fn resolve_binding(&self, abstract_name: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT concrete FROM bindings
             WHERE abstract_name = ?1
               AND kind IN ('bind','singleton','scoped','instance','bindIf','singletonIf')
               AND concrete IS NOT NULL
               AND concrete != 'Closure'
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![abstract_name], |r| r.get::<_, Option<String>>(0))?;
        Ok(rows.next().and_then(|r| r.ok()).flatten())
    }

    /// Resolve a user-defined facade class name to the concrete class it wraps.
    /// Returns the `concrete` column from a binding stored with kind = 'user_facade'.
    pub fn resolve_facade(&self, facade_class: &str) -> anyhow::Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT concrete FROM bindings WHERE abstract_name = ?1 AND kind = 'user_facade' LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![facade_class], |r| r.get::<_, Option<String>>(0))?;
        Ok(rows.next().and_then(|r| r.ok()).flatten())
    }

    /// All member names defined in stubs for `container`. Used by diagnostics
    /// to avoid "undefined member" false positives on Illuminate classes.
    pub fn stub_member_names(
        &self,
        container: &str,
    ) -> anyhow::Result<std::collections::HashSet<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT member FROM member_types WHERE container = ?1 AND file = '__stub__'",
        )?;
        let rows = stmt.query_map(params![container], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// All (member, kind, ty) tuples from stubs for `container` — powers
    /// stub-aware completions so Illuminate methods appear without a vendor index.
    pub fn stub_symbols_for(
        &self,
        container: &str,
    ) -> anyhow::Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT member, kind, ty FROM member_types
             WHERE container = ?1 AND file = '__stub__'",
        )?;
        let rows = stmt.query_map(params![container], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }
}

fn row_to_symbol(r: &rusqlite::Row<'_>) -> rusqlite::Result<Symbol> {
    let kind_str: String = r.get(2)?;
    Ok(Symbol {
        name: r.get(0)?,
        fqn: r.get(1)?,
        kind: SymbolKind::from_str(&kind_str).unwrap_or(SymbolKind::Constant),
        file: r.get(3)?,
        container: r.get(4)?,
        line: r.get::<_, i64>(5)? as u32,
        name_offset: r.get::<_, i64>(6)? as u32,
        range_start: r.get::<_, i64>(7).unwrap_or(0) as u32,
        range_end: r.get::<_, i64>(8).unwrap_or(0) as u32,
    })
}
