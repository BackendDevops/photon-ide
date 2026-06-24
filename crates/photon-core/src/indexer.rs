//! Indexer: orchestrates workspace walk → parse → store.
//!
//! v1 performs a full index on open and a per-file re-index on save (the
//! incremental fast path from `docs/04-indexing-engine.md`). Each file's
//! symbols/refs/routes/models/config/translations are replaced atomically, so
//! re-indexing one file never touches the rest.

use crate::db::Index;
use crate::types::{KeyEntry, ProjectSummary, RootInfo};
use crate::{laravel, php};
use crate::workspace::Workspace;

const ILLUMINATE_STUBS: &str = include_str!("../stubs/illuminate.json");

pub struct Engine {
    pub workspace: Workspace,
    pub index: Index,
}

impl Engine {
    /// Back-compat single-root constructor (indexes the root immediately).
    pub fn new(root: impl Into<std::path::PathBuf>, mut index: Index) -> Self {
        let _ = index.load_stubs(ILLUMINATE_STUBS);
        let mut e = Engine {
            workspace: Workspace::open(root),
            index,
        };
        let _ = e.index_all();
        e
    }

    /// Empty engine; add projects with `add_project`.
    pub fn new_empty(mut index: Index) -> Self {
        let _ = index.load_stubs(ILLUMINATE_STUBS);
        Engine {
            workspace: Workspace::new(),
            index,
        }
    }

    /// Add (and index) a project root. Cross-project navigation works because
    /// every root feeds this one shared index.
    pub fn add_project(&mut self, path: impl Into<std::path::PathBuf>) -> anyhow::Result<ProjectSummary> {
        let label = self.workspace.add_root(path.into());
        let files = self.workspace.list_files_for(&label);
        self.index.insert_files(&files)?;
        for f in &files {
            if f.is_vendor {
                continue;
            }
            if let Ok(src) = self.workspace.read_file(&f.path) {
                self.index_file_contents(&f.path, &f.lang, &src)?;
            }
        }
        self.summary()
    }

    /// Add a project root, reconciling against a (possibly persistent) index:
    /// only files that are new or whose mtime changed are re-parsed, and files
    /// that vanished from disk are dropped. On a fresh index this degrades to a
    /// full index; on a warm start it makes opening a known project near-instant.
    pub fn add_project_reconcile(
        &mut self,
        path: impl Into<std::path::PathBuf>,
    ) -> anyhow::Result<ProjectSummary> {
        let label = self.workspace.add_root(path.into());
        let current = self.workspace.list_files_for(&label);
        let stored = self.index.file_mtimes_with_prefix(&label).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();

        for f in &current {
            seen.insert(f.path.clone());
            // Unchanged & already indexed (with a real mtime) → keep as-is.
            let unchanged = f.mtime != 0
                && stored.get(&f.path).map(|m| *m == f.mtime).unwrap_or(false);
            if unchanged {
                continue;
            }
            self.index.upsert_file(f)?;
            // Vendor files are declaration-indexed separately by `index_vendor`.
            if !f.is_vendor {
                if let Ok(src) = self.workspace.read_file(&f.path) {
                    self.index_file_contents(&f.path, &f.lang, &src)?;
                }
            }
        }
        // Files indexed last time but no longer on disk.
        for path in stored.keys() {
            if !seen.contains(path) {
                self.index.delete_file_rows(path)?;
            }
        }
        self.summary()
    }

    /// Close a project root and drop all its rows from the index.
    pub fn close_project(&mut self, label: &str) -> anyhow::Result<ProjectSummary> {
        self.index.clear_root(label)?;
        self.workspace.remove_root(label);
        self.summary()
    }

    /// Declaration-level index of `vendor/` across all roots: symbols only
    /// (no references, no Laravel facts, no method bodies) so framework/package
    /// classes are navigable and searchable with bounded memory.
    ///
    /// Files are parsed **in parallel** with Rayon (CPU-bound); extracted
    /// symbols are flushed to SQLite **serially** in chunks so the DB
    /// connection is never shared across threads. Typical speedup on a
    /// real Laravel project: 3-8× depending on core count.
    pub fn index_vendor(&mut self) -> anyhow::Result<u32> {
        use rayon::prelude::*;

        // Idempotent: clear any previously-indexed vendor symbols first so a
        // warm start (persistent index) never accumulates duplicates.
        self.index.clear_vendor_symbols()?;

        let files = self.workspace.vendor_files();
        let count = files.len() as u32;

        // ── Phase 1: parallel parse ──────────────────────────────────────────
        // Read each file and extract its symbols on a rayon thread pool.
        // `workspace.read_file` only does `fs::read_to_string` — no shared
        // mutable state — so it is safe to call from multiple threads.
        let all_symbols: Vec<crate::types::Symbol> = files
            .par_iter()
            .filter_map(|f| {
                let src = self.workspace.read_file(&f.path).ok()?;
                Some(php::extract_symbols(&f.path, &src))
            })
            .flatten()
            .collect();

        // ── Phase 2: serial flush to SQLite in chunks ────────────────────────
        // SQLite connections are not Send; we write from the main thread only.
        const CHUNK: usize = 4_000;
        for chunk in all_symbols.chunks(CHUNK) {
            self.index.insert_symbols_bulk(chunk)?;
        }

        Ok(count)
    }

    pub fn projects(&self) -> Vec<RootInfo> {
        self.workspace
            .roots
            .iter()
            .map(|r| RootInfo {
                label: r.label.clone(),
                path: r.path.to_string_lossy().to_string(),
                is_laravel: crate::workspace::is_laravel_root(&r.path),
            })
            .collect()
    }

    fn summary(&self) -> anyhow::Result<ProjectSummary> {
        let is_laravel = self.workspace.is_laravel();
        self.index.set_meta("is_laravel", &is_laravel.to_string())?;
        Ok(ProjectSummary {
            root: self
                .workspace
                .primary_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            files_indexed: self.index.count("files")?,
            symbols: self.index.count("symbols")?,
            routes: self.index.count("routes")?,
            references: self.index.count("refs")?,
            models: self.index.count("models")?,
            is_laravel,
            php_files: self.index.count_php_files()?,
        })
    }

    /// Full (re)index of every open root.
    pub fn index_all(&mut self) -> anyhow::Result<ProjectSummary> {
        let files = self.workspace.list_files();
        self.index.clear()?;
        self.index.insert_files(&files)?;
        for f in &files {
            if f.is_vendor {
                continue;
            }
            if let Ok(src) = self.workspace.read_file(&f.path) {
                self.index_file_contents(&f.path, &f.lang, &src)?;
            }
        }
        self.summary()
    }

    /// Plan a Safe Rename across the project (no files changed yet).
    pub fn plan_rename(&self, old: &str, new: &str) -> anyhow::Result<crate::types::ChangeSet> {
        let ws = &self.workspace;
        crate::refactor::plan_rename(&self.index, old, new, |f: &str| ws.read_file(f).ok())
    }

    /// Plan moving a class to a new namespace (updates imports + FQ refs).
    pub fn plan_move_class(&self, class: &str, new_ns: &str) -> anyhow::Result<crate::types::ChangeSet> {
        let ws = &self.workspace;
        let files: Vec<String> = ws.list_files().into_iter().map(|f| f.path).collect();
        crate::refactor::plan_move_class(&self.index, class, new_ns, &files, |f: &str| ws.read_file(f).ok())
    }

    /// Plan changing the parameter list of the function/method whose name is on
    /// `line` in `file`.
    pub fn plan_change_signature(
        &self,
        file: &str,
        line: u32,
        new_params: &str,
    ) -> anyhow::Result<crate::types::ChangeSet> {
        let content = self.workspace.read_file(file)?;
        let decl_start = self
            .index
            .symbols_in_file(file)?
            .into_iter()
            .find(|s| {
                s.line == line
                    && matches!(s.kind, crate::types::SymbolKind::Method | crate::types::SymbolKind::Function)
            })
            .map(|s| s.range_start)
            .ok_or_else(|| anyhow::anyhow!("no function/method on line {line}"))?;
        Ok(crate::refactor::plan_change_signature(&content, file, decl_start, new_params, line))
    }

    /// Apply a previously-planned change set, then re-index the touched files.
    /// `accepted` optionally restricts which edits to apply (by index).
    pub fn apply_changeset(
        &mut self,
        cs: &crate::types::ChangeSet,
        accepted: Option<&[usize]>,
    ) -> anyhow::Result<u32> {
        let results = {
            let ws = &self.workspace;
            crate::refactor::apply_changeset(cs, accepted, |f: &str| ws.read_file(f).ok())?
        };
        let n = results.len() as u32;
        for (file, content) in results {
            self.workspace.write_file(&file, &content)?;
            self.reindex_file(&file)?;
        }
        Ok(n)
    }

    /// Re-index a single file (called on save). Cheap and isolated.
    pub fn reindex_file(&mut self, rel: &str) -> anyhow::Result<()> {
        let src = match self.workspace.read_file(rel) {
            Ok(s) => s,
            Err(_) => return Ok(()),
        };
        let lang = crate::workspace::classify(std::path::Path::new(rel));
        self.index_file_contents(rel, &lang, &src)
    }

    /// Reconcile a single workspace path after an external change (filesystem
    /// watcher). If the file exists it's (re)indexed and its `files` row is
    /// upserted (so newly-created files appear); if it's gone, all of its rows
    /// are dropped. Vendor files only get their `files` row touched.
    pub fn reindex_path(&mut self, wpath: &str) -> anyhow::Result<()> {
        match self.workspace.read_file(wpath) {
            Ok(src) => {
                if let Some(fe) = self.workspace.file_entry(wpath) {
                    self.index.upsert_file(&fe)?;
                    if fe.is_vendor {
                        return Ok(());
                    }
                }
                let lang = crate::workspace::classify(std::path::Path::new(wpath));
                self.index_file_contents(wpath, &lang, &src)
            }
            Err(_) => self.index.delete_file_rows(wpath),
        }
    }

    /// Extract and store everything we know about one file.
    fn index_file_contents(&mut self, rel: &str, lang: &str, src: &str) -> anyhow::Result<()> {
        // Laravel conventions are relative to the project root, so strip the
        // workspace label ("<label>/...") before matching paths.
        let inner = rel.split_once('/').map(|(_, r)| r).unwrap_or(rel);
        if lang == "php" || lang == "blade" {
            let symbols = php::extract_symbols(rel, src);
            // Namespace (for FQNs of models) is the first namespace symbol.
            let namespace = symbols
                .iter()
                .find(|s| s.kind == crate::types::SymbolKind::Namespace)
                .and_then(|s| s.fqn.clone());

            self.index.replace_symbols_for_file(rel, &symbols)?;
            self.index
                .replace_refs_for_file(rel, &php::extract_references(rel, src))?;
            self.index
                .replace_type_relations_for_file(rel, &php::extract_type_relations(rel, src))?;
            self.index
                .replace_member_types_for_file(rel, &php::extract_member_types(rel, src))?;

            // Laravel routes (path checks use `inner` — the label-stripped path)
            if laravel::is_routes_file(inner) {
                self.index
                    .replace_routes_for_file(rel, &laravel::extract_routes(rel, src))?;
            } else if src.contains("#[Route(") {
                // Symfony-style attribute routes on controller methods.
                let routes = laravel::extract_attribute_routes(rel, src);
                if !routes.is_empty() {
                    self.index.replace_routes_for_file(rel, &routes)?;
                }
            }
            // Eloquent models
            let models = laravel::extract_models(rel, src, namespace.as_deref());
            if !models.is_empty() {
                self.index.replace_models_for_file(rel, &models)?;
            }
            // Container bindings, events, jobs, factories/seeders, user facades
            let mut bindings = laravel::extract_bindings(rel, src);
            bindings.extend(laravel::extract_user_facades(rel, src));
            let events = laravel::extract_events(rel, src);
            let jobs = laravel::extract_jobs(rel, src);
            let artifacts = laravel::extract_artifacts(rel, src);
            if !bindings.is_empty() || !events.is_empty() || !jobs.is_empty() || !artifacts.is_empty()
            {
                self.index
                    .replace_laravel_facts_for_file(rel, &bindings, &events, &jobs, &artifacts)?;
            }
            // Migration columns (real DB columns for Eloquent completion)
            if inner.contains("database/migrations/") {
                let cols = laravel::extract_migration_columns(src);
                if !cols.is_empty() {
                    self.index.replace_mig_columns_for_file(rel, &cols)?;
                }
            }
            // Config keys
            if let Some(prefix) = config_prefix(inner) {
                self.index
                    .replace_config_keys_for_file(rel, &laravel::extract_config_keys(rel, src, &prefix))?;
            }
            // PHP translation files
            if let Some((prefix, locale)) = lang_php_meta(inner) {
                let keys = laravel::extract_translations(rel, src, &prefix, &locale, false);
                self.index.replace_translations_for_file(rel, &keys)?;
            }
        } else if lang == "json" {
            // JSON translation file: lang/<locale>.json
            if let Some(locale) = lang_json_locale(inner) {
                let keys: Vec<KeyEntry> = laravel::extract_translations(rel, src, "", &locale, true);
                self.index.replace_translations_for_file(rel, &keys)?;
            }
        }
        Ok(())
    }
}

/// `config/services.php` → Some("services").
fn config_prefix(rel: &str) -> Option<String> {
    if rel.starts_with("config/") && rel.ends_with(".php") {
        let base = rel.trim_start_matches("config/").trim_end_matches(".php");
        // nested config dirs become dotted prefixes
        return Some(base.replace('/', "."));
    }
    None
}

/// `lang/en/auth.php` or `resources/lang/en/auth.php` → ("auth", "en").
fn lang_php_meta(rel: &str) -> Option<(String, String)> {
    let stripped = rel
        .strip_prefix("lang/")
        .or_else(|| rel.strip_prefix("resources/lang/"))?;
    if !stripped.ends_with(".php") {
        return None;
    }
    let parts: Vec<&str> = stripped.trim_end_matches(".php").split('/').collect();
    if parts.len() < 2 {
        return None;
    }
    let locale = parts[0].to_string();
    let prefix = parts[1..].join("/").replace('/', ".");
    Some((prefix, locale))
}

/// `lang/en.json` or `resources/lang/en.json` → "en".
fn lang_json_locale(rel: &str) -> Option<String> {
    let stripped = rel
        .strip_prefix("lang/")
        .or_else(|| rel.strip_prefix("resources/lang/"))?;
    if stripped.ends_with(".json") && !stripped.contains('/') {
        return Some(stripped.trim_end_matches(".json").to_string());
    }
    None
}
