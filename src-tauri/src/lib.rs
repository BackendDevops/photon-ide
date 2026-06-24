//! Photon IDE desktop shell.
//!
//! Thin Tauri layer over `photon-core` plus the v1 runtime services (database
//! tools and git). All heavy logic lives in `photon-core` / the service
//! modules; this file is command glue and shared state.

mod ai;
mod dbtools;
mod redis_client;
mod debugger;
mod extensions;
mod git;
mod templates;
mod terminal;

use dbtools::{DataSource, DbManager, DbSchema, QueryResult};
use extensions::{ExtensionInfo, Snippet};
use std::collections::HashMap;
use templates::Template;
use photon_core::{
    ChangeSet, Engine, FileEntry, Index, KeyEntry, MissingTranslation, ModelInfo, ProjectSummary,
    Reference, Route, SearchHit, Symbol,
};
use parking_lot::Mutex;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, State};
use terminal::Terminals;

#[derive(Default)]
struct AppState {
    engine: Mutex<Option<Engine>>,
    /// Live filesystem watchers, keyed by project label (kept alive here).
    watchers: Mutex<std::collections::HashMap<String, notify::RecommendedWatcher>>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn project_root(state: &State<'_, AppState>) -> CmdResult<String> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    engine
        .workspace
        .primary_path()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "No project open".to_string())
}

// ------------------------- project & files -------------------------

/// Bumped whenever the index schema/extraction changes in a way that makes a
/// previously-persisted index unsafe to reuse (forces a one-time full reindex).
const INDEX_SCHEMA_VERSION: i64 = 1;

/// Open the on-disk index for a project at `<root>/.photon/index.sqlite`,
/// falling back to an in-memory index if the file can't be opened. The schema
/// gate wipes a stale-shaped index so warm-start reconcile is always safe.
fn open_persistent_index(root: &str) -> Index {
    let dir = std::path::Path::new(root).join(".photon");
    let _ = std::fs::create_dir_all(&dir);
    let index = dir
        .join("index.sqlite")
        .to_str()
        .and_then(|p| Index::open(p).ok())
        .or_else(|| Index::open(":memory:").ok())
        .expect("an in-memory index can always be opened");
    let _ = index.ensure_schema_version(INDEX_SCHEMA_VERSION);
    index
}

/// Open a folder. The first call creates the workspace (and its persistent
/// index); subsequent calls ADD the folder as another root (multiple projects
/// open at once, one shared index → cross-project navigation). Opening a
/// previously-indexed project is a fast warm start: only changed files re-parse.
#[tauri::command]
fn open_project(
    app: AppHandle,
    path: String,
    state: State<'_, AppState>,
) -> CmdResult<ProjectSummary> {
    let summary = {
        let mut guard = state.engine.lock();
        match guard.as_mut() {
            Some(engine) => engine.add_project_reconcile(path.clone()).map_err(err)?,
            None => {
                let index = open_persistent_index(&path);
                let mut engine = Engine::new_empty(index);
                let s = engine.add_project_reconcile(path.clone()).map_err(err)?;
                *guard = Some(engine);
                s
            }
        }
    };
    start_root_watcher(&app, &state, &path);
    Ok(summary)
}

/// Re-index a single workspace path after an external filesystem change.
#[tauri::command]
fn reindex_path(path: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    engine.reindex_path(&path).map_err(err)
}

/// Files/dirs we never react to (index churn, VCS, dependencies, build output).
fn watcher_ignored(rel: &str) -> bool {
    rel.split('/').any(|seg| {
        matches!(
            seg,
            ".git" | "node_modules" | ".photon" | "target" | "dist" | "build" | ".idea" | ".vscode"
        )
    }) || rel.contains("vendor/")
        || rel.contains("storage/framework/")
        || rel.contains("bootstrap/cache/")
}

/// Map a raw filesystem event to the workspace paths (`<label>/<rel>`) we index.
fn watcher_event_paths(
    res: notify::Result<notify::Event>,
    root: &std::path::Path,
    label: &str,
) -> Vec<String> {
    const KEEP: &[&str] = &[
        ".php", ".json", ".js", ".mjs", ".cjs", ".ts", ".tsx", ".jsx", ".vue", ".env",
        ".sql", ".md", ".css", ".html", ".yml", ".yaml",
    ];
    let event = match res {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for p in event.paths {
        let rel = match p.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        if rel.is_empty() || watcher_ignored(&rel) {
            continue;
        }
        let base = rel.rsplit('/').next().unwrap_or(&rel);
        let keep = KEEP.iter().any(|e| rel.ends_with(e))
            || matches!(base, "artisan" | "composer.json")
            || base.starts_with(".env");
        if keep {
            out.push(format!("{label}/{rel}"));
        }
    }
    out
}

/// Start a recursive filesystem watcher for the project root, emitting debounced
/// `fs-changed` events (a list of workspace paths) so the UI can re-index and
/// refresh. No-op if a watcher for this label already exists.
fn start_root_watcher(app: &AppHandle, state: &State<'_, AppState>, path: &str) {
    use notify::{RecursiveMode, Watcher};

    let label = {
        let guard = state.engine.lock();
        match guard.as_ref() {
            Some(engine) => engine.projects().into_iter().find(|r| r.path == *path).map(|r| r.label),
            None => None,
        }
    };
    let label = match label {
        Some(l) => l,
        None => return,
    };
    {
        let watchers = state.watchers.lock();
        if watchers.contains_key(&label) {
            return;
        }
    }

    let root = std::path::PathBuf::from(path);
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = match notify::RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        notify::Config::default(),
    ) {
        Ok(w) => w,
        Err(_) => return,
    };
    if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
        return;
    }
    state.watchers.lock().insert(label.clone(), watcher);

    let app = app.clone();
    std::thread::spawn(move || {
        use std::time::{Duration, Instant};
        while let Ok(first) = rx.recv() {
            let mut paths = watcher_event_paths(first, &root, &label);
            // Debounce: coalesce a burst (editor saves, git ops) for ~300ms.
            let deadline = Instant::now() + Duration::from_millis(300);
            loop {
                let now = Instant::now();
                if now >= deadline {
                    break;
                }
                match rx.recv_timeout(deadline - now) {
                    Ok(ev) => paths.extend(watcher_event_paths(ev, &root, &label)),
                    Err(_) => break,
                }
            }
            paths.sort();
            paths.dedup();
            if !paths.is_empty() {
                let _ = app.emit("fs-changed", &paths);
            }
        }
    });
}

#[tauri::command]
fn close_project(label: String, state: State<'_, AppState>) -> CmdResult<ProjectSummary> {
    // Dropping the watcher disconnects its channel; its thread then exits.
    state.watchers.lock().remove(&label);
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    engine.close_project(&label).map_err(err)
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<photon_core::RootInfo>> {
    let guard = state.engine.lock();
    Ok(guard.as_ref().map(|e| e.projects()).unwrap_or_default())
}

/// Declaration-level index of vendor/ (framework + packages). Called by the UI
/// AFTER open_project so initial open stays fast; runs on a worker thread.
#[tauri::command]
fn index_vendor(state: State<'_, AppState>) -> CmdResult<u32> {
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    engine.index_vendor().map_err(err)
}

#[tauri::command]
fn list_files(state: State<'_, AppState>) -> CmdResult<Vec<FileEntry>> {
    let guard = state.engine.lock();
    Ok(guard.as_ref().ok_or("No project open")?.workspace.list_files())
}

#[tauri::command]
fn read_file(path: String, state: State<'_, AppState>) -> CmdResult<String> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .workspace
        .read_file(&path)
        .map_err(err)
}

#[tauri::command]
fn save_file(path: String, contents: String, state: State<'_, AppState>) -> CmdResult<()> {
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    engine.workspace.write_file(&path, &contents).map_err(err)?;
    engine.reindex_file(&path).map_err(err)?;
    // Local History: snapshot this version (Git-independent timeline).
    snapshot_save(engine, &path, &contents);
    Ok(())
}

// ------------------------- Local History (docs/19 — power tools) -------------
// Git-independent, timestamped snapshots under `<primary>/.photon/history/`.

fn history_root(engine: &Engine) -> Option<std::path::PathBuf> {
    engine.workspace.primary_path().map(|p| p.join(".photon").join("history"))
}

fn encode_path(wpath: &str) -> String {
    wpath
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '.' { c } else { '_' })
        .collect()
}

/// Write a snapshot if the content differs from the most recent one; keep the
/// last 50 per file so the store stays bounded.
fn snapshot_save(engine: &Engine, wpath: &str, contents: &str) {
    let dir = match history_root(engine) {
        Some(r) => r.join(encode_path(wpath)),
        None => return,
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // Collect existing snapshots (sorted by timestamp in the filename).
    let mut snaps: Vec<(i64, std::path::PathBuf)> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    let ts = p.file_stem()?.to_str()?.parse::<i64>().ok()?;
                    Some((ts, p))
                })
                .collect()
        })
        .unwrap_or_default();
    snaps.sort_by_key(|(ts, _)| *ts);
    // Dedup: skip if identical to the latest snapshot.
    if let Some((_, last)) = snaps.last() {
        if std::fs::read_to_string(last).map(|c| c == contents).unwrap_or(false) {
            return;
        }
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let _ = std::fs::write(dir.join(format!("{ts}.snap")), contents);
    // Prune to the newest 50.
    if snaps.len() + 1 > 50 {
        let remove = snaps.len() + 1 - 50;
        for (_, p) in snaps.into_iter().take(remove) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Snapshot timestamps (ms) for a file, newest first.
#[tauri::command]
fn history_list(path: String, state: State<'_, AppState>) -> CmdResult<Vec<i64>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let dir = match history_root(engine) {
        Some(r) => r.join(encode_path(&path)),
        None => return Ok(vec![]),
    };
    let mut out: Vec<i64> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.path().file_stem()?.to_str()?.parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_unstable_by(|a, b| b.cmp(a));
    Ok(out)
}

/// The content of one snapshot.
#[tauri::command]
fn history_get(path: String, ts: i64, state: State<'_, AppState>) -> CmdResult<String> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let dir = history_root(engine).ok_or("No project open")?.join(encode_path(&path));
    std::fs::read_to_string(dir.join(format!("{ts}.snap"))).map_err(err)
}

#[tauri::command]
fn file_symbols(path: String, state: State<'_, AppState>) -> CmdResult<Vec<Symbol>> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .index
        .symbols_in_file(&path)
        .map_err(err)
}

#[tauri::command]
fn search_everywhere(query: String, state: State<'_, AppState>) -> CmdResult<Vec<SearchHit>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    Ok(photon_core::search::search_everywhere(&engine.index, &query, 50))
}

#[tauri::command]
fn list_routes(state: State<'_, AppState>) -> CmdResult<Vec<Route>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.routes().map_err(err)
}

// ------------------------- navigation & refactoring -------------------------

#[tauri::command]
fn goto_symbol(name: String, state: State<'_, AppState>) -> CmdResult<Vec<Symbol>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.find_symbol(&name).map_err(err)
}

#[tauri::command]
fn find_usages(name: String, state: State<'_, AppState>) -> CmdResult<Vec<Reference>> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .index
        .references_to(&name)
        .map_err(err)
}

#[tauri::command]
fn plan_rename(old: String, new_name: String, state: State<'_, AppState>) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .plan_rename(&old, &new_name)
        .map_err(err)
}

#[tauri::command]
fn plan_move_class(class: String, new_ns: String, state: State<'_, AppState>) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .plan_move_class(&class, &new_ns)
        .map_err(err)
}

#[tauri::command]
fn plan_change_signature(
    file: String,
    line: u32,
    new_params: String,
    state: State<'_, AppState>,
) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .plan_change_signature(&file, line, &new_params)
        .map_err(err)
}

/// PSR-4 autoload prefixes → directories from the primary project's composer.json.
#[tauri::command]
fn psr4_map(state: State<'_, AppState>) -> CmdResult<Vec<(String, String)>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let root = match engine.workspace.primary_path() {
        Some(r) => r.to_path_buf(),
        None => return Ok(vec![]),
    };
    let txt = match std::fs::read_to_string(root.join("composer.json")) {
        Ok(t) => t,
        Err(_) => return Ok(vec![]),
    };
    let json: serde_json::Value = serde_json::from_str(&txt).map_err(err)?;
    let mut out = Vec::new();
    for section in ["autoload", "autoload-dev"] {
        if let Some(map) = json
            .get(section)
            .and_then(|a| a.get("psr-4"))
            .and_then(|p| p.as_object())
        {
            for (prefix, dir) in map {
                let dir = match dir {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Array(a) => {
                        a.first().and_then(|v| v.as_str()).unwrap_or("").to_string()
                    }
                    _ => String::new(),
                };
                out.push((prefix.clone(), dir));
            }
        }
    }
    Ok(out)
}

#[tauri::command]
fn apply_rename(
    changeset: ChangeSet,
    accepted: Option<Vec<usize>>,
    state: State<'_, AppState>,
) -> CmdResult<u32> {
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    engine
        .apply_changeset(&changeset, accepted.as_deref())
        .map_err(err)
}

#[tauri::command]
fn refactor_extract_variable(
    file: String,
    sel_start: u32,
    sel_end: u32,
    new_name: String,
    line: u32,
    state: State<'_, AppState>,
) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let content = engine.workspace.read_file(&file).map_err(err)?;
    Ok(photon_core::refactor::plan_extract_variable(
        &content, &file, sel_start, sel_end, &new_name, line,
    ))
}

#[tauri::command]
fn refactor_inline_variable(
    file: String,
    var: String,
    state: State<'_, AppState>,
) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let content = engine.workspace.read_file(&file).map_err(err)?;
    Ok(photon_core::refactor::plan_inline_variable(&content, &file, &var))
}

#[tauri::command]
fn refactor_safe_delete(name: String, state: State<'_, AppState>) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let refs = engine.index.references_to(&name).map_err(err)?;
    if !refs.is_empty() {
        return Err(format!(
            "{} usage(s) found — resolve them before deleting (use Find Usages)",
            refs.len()
        ));
    }
    let defs = engine.index.find_symbol(&name).map_err(err)?;
    let def = defs.first().ok_or("symbol not found")?;
    let content = engine.workspace.read_file(&def.file).map_err(err)?;

    // Prefer the full declaration range (v2 body-range index); fall back to the
    // single definition line for older indexes.
    let (mut start, mut end) = if def.range_end > def.range_start {
        (def.range_start as usize, def.range_end as usize)
    } else {
        let mut ls = 0usize;
        let (mut s, mut e) = (0usize, 0usize);
        for (i, l) in content.split_inclusive('\n').enumerate() {
            if (i as u32) + 1 == def.line {
                s = ls;
                e = ls + l.len();
                break;
            }
            ls += l.len();
        }
        (s, e)
    };
    // Extend to whole lines: back to line start, forward past the trailing newline.
    start = content[..start.min(content.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    if let Some(nl) = content[end.min(content.len())..].find('\n') {
        end = end + nl + 1;
    }
    let snippet = content.get(start..end.min(content.len())).unwrap_or("");
    let preview = snippet.lines().next().unwrap_or("").trim().to_string();
    let edit = photon_core::TextEdit {
        file: def.file.clone(),
        start: start as u32,
        end: end as u32,
        line: def.line,
        new_text: String::new(),
        preview: format!("(remove) {} …", preview),
        certain: true,
    };
    Ok(ChangeSet {
        title: format!("Safe delete {} ({})", name, def.kind.as_str()),
        files_affected: 1,
        edits: vec![edit],
    })
}

#[tauri::command]
fn refactor_extract_method(
    file: String,
    sel_start: u32,
    sel_end: u32,
    method_name: String,
    line: u32,
    state: State<'_, AppState>,
) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let content = engine.workspace.read_file(&file).map_err(err)?;
    let symbols = engine.index.symbols_in_file(&file).unwrap_or_default();
    // Enclosing method = innermost Method whose range covers the selection.
    let m = symbols
        .iter()
        .filter(|s| matches!(s.kind, photon_core::SymbolKind::Method))
        .filter(|s| s.range_end > s.range_start && s.range_start <= sel_start && sel_end <= s.range_end)
        .min_by_key(|s| s.range_end - s.range_start);
    let (insert_at, indent) = match m {
        Some(m) => {
            let ls = content[..(m.range_start as usize).min(content.len())]
                .rfind('\n')
                .map(|i| i + 1)
                .unwrap_or(0);
            let indent: String = content[ls..]
                .chars()
                .take_while(|c| *c == ' ' || *c == '\t')
                .collect();
            (m.range_end, indent)
        }
        None => (sel_end, "    ".to_string()),
    };
    Ok(photon_core::refactor::plan_extract_method(
        &content, &file, sel_start, sel_end, &method_name, insert_at, &indent, line,
    ))
}

// ------------------------- Laravel Idea Wave 3 -------------------------

/// Generate a model PHPDoc (`@property` / `@property-read`) from its columns
/// (typed, from migrations) and relations, inserted above the class.
#[tauri::command]
fn generate_model_phpdoc(file: String, state: State<'_, AppState>) -> CmdResult<ChangeSet> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let m = engine
        .index
        .models()
        .unwrap_or_default()
        .into_iter()
        .find(|m| m.file == file)
        .ok_or("No Eloquent model in this file")?;

    let table = m.table.clone().unwrap_or_default();
    let mut cols = if table.is_empty() {
        Vec::new()
    } else {
        engine.index.columns_with_types(&table).unwrap_or_default()
    };
    if cols.is_empty() {
        cols = m.fillable.iter().map(|c| (c.clone(), "mixed".to_string())).collect();
    }

    let ns = m
        .fqn
        .as_deref()
        .and_then(|f| f.rsplit_once('\\').map(|(n, _)| n.to_string()))
        .unwrap_or_else(|| "App\\Models".to_string());

    let mut lines = vec!["/**".to_string()];
    for (col, ty) in &cols {
        lines.push(format!(" * @property {} ${}", ty, col));
    }
    for rel in &m.relations {
        let related = rel.related.clone().unwrap_or_else(|| "Model".to_string());
        let fqn = format!("\\{}\\{}", ns, related);
        let many = matches!(
            rel.rel_type.as_str(),
            "hasMany" | "belongsToMany" | "morphMany" | "morphToMany" | "hasManyThrough"
        );
        if many {
            lines.push(format!(
                " * @property-read \\Illuminate\\Database\\Eloquent\\Collection<int, {}> ${}",
                fqn, rel.method
            ));
        } else {
            lines.push(format!(" * @property-read {} ${}", fqn, rel.method));
        }
    }
    lines.push(" */".to_string());
    let docblock = lines.join("\n");

    let content = engine.workspace.read_file(&file).map_err(err)?;
    let def = engine
        .index
        .find_symbol(&m.name)
        .unwrap_or_default()
        .into_iter()
        .find(|s| s.file == file && matches!(s.kind, photon_core::SymbolKind::Class))
        .ok_or("class declaration not found")?;
    let class_off = def.range_start as usize;
    let line_start = content[..class_off.min(content.len())]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    // Replace an existing @property docblock right above, else insert.
    let before = &content[..line_start];
    let (start, end) = {
        let trimmed_len = before.trim_end().len();
        if before.trim_end().ends_with("*/") {
            if let Some(open) = before[..trimmed_len].rfind("/**") {
                if content[open..line_start].contains("@property") {
                    (open, line_start)
                } else {
                    (line_start, line_start)
                }
            } else {
                (line_start, line_start)
            }
        } else {
            (line_start, line_start)
        }
    };

    Ok(ChangeSet {
        title: format!("Generate PHPDoc for {}", m.name),
        files_affected: 1,
        edits: vec![photon_core::TextEdit {
            file: file.clone(),
            start: start as u32,
            end: end as u32,
            line: def.line,
            new_text: format!("{}\n", docblock),
            preview: format!("@property block · {} columns", cols.len()),
            certain: true,
        }],
    })
}

#[tauri::command]
fn artisan_commands(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let root = project_root(&state)?;
    let out = std::process::Command::new("php")
        .args(["artisan", "list", "--raw"])
        .current_dir(&root)
        .output()
        .map_err(|e| format!("failed to run php artisan: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string())
        .collect())
}

#[derive(serde::Serialize)]
struct TestResult {
    passed: bool,
    output: String,
}

/// Run a test file/method via Pest (if present) or PHPUnit, from the Rust core.
#[tauri::command]
fn run_test(path: String, filter: Option<String>, state: State<'_, AppState>) -> CmdResult<TestResult> {
    let root = project_root(&state)?;
    let rel = path.split_once('/').map(|(_, r)| r.to_string()).unwrap_or(path);
    let pest = std::path::Path::new(&root).join("vendor/bin/pest").exists();
    let bin = if pest { "vendor/bin/pest" } else { "vendor/bin/phpunit" };
    let mut args: Vec<String> = vec![rel];
    if let Some(f) = filter.filter(|f| !f.is_empty()) {
        args.push("--filter".into());
        args.push(f);
    }
    let out = std::process::Command::new(bin)
        .args(&args)
        .current_dir(&root)
        .output()
        .map_err(|e| format!("failed to run {bin}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    let errs = String::from_utf8_lossy(&out.stderr);
    if !errs.trim().is_empty() {
        text.push_str("\n");
        text.push_str(&errs);
    }
    Ok(TestResult { passed: out.status.success(), output: text })
}

#[tauri::command]
fn run_artisan(args: String, state: State<'_, AppState>) -> CmdResult<String> {
    let root = project_root(&state)?;
    let parts: Vec<&str> = args.split_whitespace().collect();
    let out = std::process::Command::new("php")
        .arg("artisan")
        .args(&parts)
        .current_dir(&root)
        .output()
        .map_err(|e| format!("failed to run php artisan: {e}"))?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    let errs = String::from_utf8_lossy(&out.stderr);
    if !errs.trim().is_empty() {
        s.push_str(&errs);
    }
    Ok(s)
}

// ------------------------- type intelligence (v2 W1) -------------------------

/// Resolve a member-access chain to a class name, walking declared member
/// types (`$this->svc->find()` → the find() return type). Shared by member
/// completion and receiver-aware go-to-definition.
fn resolve_chain_class(engine: &photon_core::Engine, file: &str, offset: u32, chain: &str) -> Option<String> {
    let source = engine.workspace.read_file(file).unwrap_or_default();
    let symbols = engine.index.symbols_in_file(file).unwrap_or_default();
    let segs = split_chain(chain);
    let model_names: std::collections::HashSet<String> =
        engine.index.model_names().unwrap_or_default().into_iter().collect();

    let mut cur: Option<String> = segs.first().map(|(root, _)| root.clone()).and_then(|root| {
        if matches!(root.as_str(), "$this" | "self" | "static" | "parent") {
            photon_core::infer::enclosing_class(&symbols, offset)
        } else if root == "auth" {
            Some("User".to_string())
        } else if root.starts_with('$') {
            photon_core::infer::infer_var_type(&source, &root)
        } else {
            Some(root.trim_end_matches("::").to_string())
        }
    });

    let mut elem: Option<String> = None; // element type of the last collection member
    for (name, _is_call) in segs.iter().skip(1) {
        let class = match &cur {
            Some(c) if !c.is_empty() => c.clone(),
            _ => break,
        };
        // Collection element access: `$user->orders->first()` → element type.
        if let Some(e) = elem.take() {
            if ELEMENT_ACCESSORS.contains(&name.as_str()) {
                cur = Some(e);
                continue;
            }
        }
        // Eloquent return-type refinement: a model's builder methods resolve to
        // the right shape so completion after each step is accurate.
        if model_names.contains(&class) {
            let n = name.as_str();
            if COLLECTION_RETURNING.contains(&n) {
                cur = Some("Collection".to_string());
                continue;
            }
            if MODEL_RETURNING.contains(&n) {
                cur = Some(class); // the model itself
                continue;
            }
            if SCALAR_TERMINATING.contains(&n) {
                cur = None; // int/bool/string — chain ends
                break;
            }
            if ELOQUENT_BUILDER.contains(&n) || n.starts_with("where") || n.starts_with("orWhere") {
                cur = Some(class); // still a builder over the model
                continue;
            }
            // else: a real model method/relation/docblock member → fall through.
        }
        // Collection fluent chain stays a Collection (element type unknown).
        if class == "Collection" && COLLECTION_FLUENT.contains(&name.as_str()) {
            cur = Some("Collection".to_string());
            continue;
        }
        // Remember this member's documented element type so the *next* segment
        // (`->first()`, `->find()`, …) can resolve to it.
        elem = engine.index.member_element_type(&class, name).ok().flatten();
        cur = match engine.index.member_type(&class, name).ok().flatten() {
            Some(t) if matches!(t.as_str(), "self" | "static" | "$this") => Some(class),
            Some(t) => Some(t),
            None => None,
        };
    }
    cur.filter(|c| !c.is_empty())
}

/// Collection accessors that return a single element (used with a documented
/// generic element type to resolve `$collection->first()` etc.).
const ELEMENT_ACCESSORS: &[&str] = &[
    "first", "firstOrFail", "last", "sole", "find", "pop", "shift", "get", "pull",
];

/// Receiver-aware go-to-definition for a member access: resolve the chain to a
/// class, then find the declaring class (own or up the extends/trait chain) and
/// return the member's symbol location. Falls back to any same-named symbol.
#[tauri::command]
fn goto_member_def(
    file: String,
    offset: u32,
    chain: String,
    member: String,
    state: State<'_, AppState>,
) -> CmdResult<Option<photon_core::Location>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    if let Some(class) = resolve_chain_class(engine, &file, offset, &chain) {
        let mut queue = vec![class];
        let mut visited = std::collections::HashSet::new();
        let mut budget = 0;
        while let Some(c) = queue.pop() {
            if !visited.insert(c.clone()) || budget > 12 {
                continue;
            }
            budget += 1;
            if let Some(sym) = engine
                .index
                .members_of(&c)
                .unwrap_or_default()
                .into_iter()
                .find(|s| s.name == member)
            {
                return Ok(Some(photon_core::Location { file: sym.file, line: sym.line }));
            }
            for sup in engine.index.supertypes(&c).unwrap_or_default() {
                queue.push(sup);
            }
        }
    }
    // Fallback: any symbol with this name.
    Ok(engine
        .index
        .find_symbol(&member)
        .unwrap_or_default()
        .into_iter()
        .next()
        .map(|s| photon_core::Location { file: s.file, line: s.line }))
}

/// Go to Type Definition: resolve the type of the expression at the cursor and
/// return the defining class's location.
#[tauri::command]
fn goto_type(
    file: String,
    offset: u32,
    chain: String,
    state: State<'_, AppState>,
) -> CmdResult<Option<photon_core::Location>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let class = match resolve_chain_class(engine, &file, offset, &chain) {
        Some(c) => c,
        None => return Ok(None),
    };
    Ok(engine
        .index
        .find_symbol(&class)
        .unwrap_or_default()
        .into_iter()
        .find(|s| {
            matches!(
                s.kind,
                photon_core::SymbolKind::Class
                    | photon_core::SymbolKind::Interface
                    | photon_core::SymbolKind::Trait
                    | photon_core::SymbolKind::Enum
            )
        })
        .map(|s| photon_core::Location { file: s.file, line: s.line }))
}

#[tauri::command]
fn member_completions(
    file: String,
    offset: u32,
    receiver: String,
    state: State<'_, AppState>,
) -> CmdResult<Vec<Symbol>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;

    let class = match resolve_chain_class(engine, &file, offset, &receiver) {
        Some(c) => c,
        None => return Ok(vec![]),
    };
    // Direct members + up to a few levels of inheritance.
    let mut out: Vec<Symbol> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = Some(class.clone());
    let mut depth = 0;
    while let Some(c) = current.take() {
        if depth >= 4 || !seen.insert(c.clone()) {
            break;
        }
        depth += 1;
        out.extend(engine.index.members_of(&c).unwrap_or_default());
        // resolve parent (extends) by reading the class's own file
        current = engine
            .index
            .find_symbol(&c)
            .ok()
            .and_then(|defs| defs.into_iter().next())
            .and_then(|def| engine.workspace.read_file(&def.file).ok())
            .and_then(|src| class_parent(&src, &c));
    }
    // Eloquent awareness (Laravel-Idea style): if the receiver is a model,
    // also offer query-builder methods, the model's columns, and relations —
    // so `User::query()->where(...)->` completes correctly.
    if let Ok(models) = engine.index.models() {
        if let Some(m) = models.into_iter().find(|m| m.name == class) {
            let synth = |name: &str, kind: photon_core::SymbolKind, container: &str| photon_core::Symbol {
                name: name.to_string(),
                fqn: None,
                kind,
                file: m.file.clone(),
                container: Some(container.to_string()),
                line: m.line,
                name_offset: 0,
                range_start: 0,
                range_end: 0,
            };
            // Prefer real columns from migrations; fall back to $fillable.
            let table = m.table.clone().unwrap_or_default();
            let mut columns = if table.is_empty() {
                Vec::new()
            } else {
                engine.index.columns_for_table(&table).unwrap_or_default()
            };
            if columns.is_empty() {
                columns = m.fillable.clone();
            }
            for col in &columns {
                out.push(synth(col, photon_core::SymbolKind::Property, &class));
                // dynamic where: whereEmail(), whereName(), …
                out.push(synth(
                    &format!("where{}", studly(col)),
                    photon_core::SymbolKind::Method,
                    "Builder",
                ));
            }
            for rel in &m.relations {
                out.push(synth(&rel.method, photon_core::SymbolKind::Method, &class));
            }
            for bm in ELOQUENT_BUILDER {
                out.push(synth(bm, photon_core::SymbolKind::Method, "Builder"));
            }
            // Local scopes: scopeActive() → usable as ->active()
            let scopes: Vec<String> = out
                .iter()
                .filter(|s| {
                    matches!(s.kind, photon_core::SymbolKind::Method)
                        && s.name.starts_with("scope")
                        && s.name.len() > 5
                })
                .map(|s| {
                    let rest = &s.name[5..];
                    let mut c = rest.chars();
                    match c.next() {
                        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
                        None => String::new(),
                    }
                })
                .filter(|s| !s.is_empty())
                .collect();
            for sc in scopes {
                out.push(synth(&sc, photon_core::SymbolKind::Method, "scope"));
            }
        }
    }

    // Enum awareness (PHP 8.1): `->value`/`->name` + `::from/tryFrom/cases`.
    let is_enum = engine
        .index
        .find_symbol(&class)
        .unwrap_or_default()
        .iter()
        .any(|s| s.name == class && matches!(s.kind, photon_core::SymbolKind::Enum));
    if is_enum {
        let mk = |name: &str, kind: photon_core::SymbolKind| photon_core::Symbol {
            name: name.to_string(),
            fqn: None,
            kind,
            file: String::new(),
            container: Some(class.clone()),
            line: 0,
            name_offset: 0,
            range_start: 0,
            range_end: 0,
        };
        out.push(mk("value", photon_core::SymbolKind::Property));
        out.push(mk("name", photon_core::SymbolKind::Property));
        for m in ["from", "tryFrom", "cases"] {
            out.push(mk(m, photon_core::SymbolKind::Method));
        }
    }

    // de-dupe by name+kind
    let mut keys = std::collections::HashSet::new();
    out.retain(|s| keys.insert((s.name.clone(), s.kind.as_str())));
    Ok(out)
}

fn studly(s: &str) -> String {
    s.split(|c| c == '_' || c == '-')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Split a member-access chain into `(name, is_call)` segments, splitting on
/// `->` / `::` at paren depth 0 (so args don't break the chain). The first
/// segment is the root (`$this`, `$var`, `ClassName`, `auth`).
fn split_chain(chain: &str) -> Vec<(String, bool)> {
    let chars: Vec<char> = chain.trim().chars().collect();
    let mut segs: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if depth == 0
            && ((c == '-' && chars.get(i + 1) == Some(&'>'))
                || (c == ':' && chars.get(i + 1) == Some(&':')))
        {
            // Nullsafe `?->`: drop the trailing `?` left on the receiver segment.
            if cur.ends_with('?') {
                cur.pop();
            }
            segs.push(std::mem::take(&mut cur));
            i += 2;
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        cur.push(c);
        i += 1;
    }
    segs.push(cur);
    segs.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| match s.find('(') {
            Some(p) => (s[..p].trim().to_string(), true),
            None => (s, false),
        })
        .collect()
}

/// Common Eloquent query-builder / collection methods offered on models.
const ELOQUENT_BUILDER: &[&str] = &[
    "query", "where", "orWhere", "whereIn", "whereNotIn", "whereNull", "whereNotNull",
    "whereHas", "whereBetween", "orderBy", "orderByDesc", "latest", "oldest", "groupBy",
    "having", "limit", "offset", "take", "skip", "with", "withCount", "has",
    "first", "firstOrFail", "firstOrCreate", "find", "findOrFail", "get", "pluck",
    "count", "exists", "paginate", "simplePaginate", "create", "update", "delete",
    "updateOrCreate", "value", "sum", "avg", "max", "min", "chunk", "each", "toSql",
];

// Builder methods that yield a Collection (so completion offers Collection ops).
const COLLECTION_RETURNING: &[&str] = &[
    "get", "all", "pluck", "paginate", "simplePaginate", "cursor", "lazy", "keyBy",
];
// Builder methods that yield a single model instance.
const MODEL_RETURNING: &[&str] = &[
    "first", "firstOrFail", "firstOrCreate", "firstOrNew", "find", "findOrFail",
    "sole", "create", "make", "updateOrCreate", "fresh", "refresh", "save",
];
// Builder methods that yield a scalar/bool — the navigation chain ends here.
const SCALAR_TERMINATING: &[&str] = &[
    "count", "exists", "doesntExist", "sum", "avg", "max", "min", "value", "toSql",
    "update", "delete", "increment", "decrement", "insert",
];
// Collection methods that return another Collection (fluent chaining).
const COLLECTION_FLUENT: &[&str] = &[
    "map", "mapWithKeys", "filter", "reject", "where", "whereIn", "sortBy",
    "sortByDesc", "sort", "values", "keys", "unique", "merge", "concat", "push",
    "put", "take", "slice", "chunk", "groupBy", "flatten", "pluck", "reverse",
    "load", "each", "tap", "fresh",
];

/// Parse `class <name> ... extends <Parent>` and return the short parent name.
fn class_parent(source: &str, class: &str) -> Option<String> {
    let needle = format!("class {}", class);
    let at = source.find(&needle)?;
    let rest = &source[at..];
    let header_end = rest.find('{').unwrap_or(rest.len().min(300));
    let header = &rest[..header_end];
    let e = header.find("extends")?;
    let parent: String = header[e + 7..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
        .collect();
    let short = parent.rsplit('\\').next().unwrap_or(&parent).to_string();
    if short.is_empty() {
        None
    } else {
        Some(short)
    }
}

/// Types that implement/extend `name`, as a usages-popup result.
#[tauri::command]
fn goto_implementations(name: String, state: State<'_, AppState>) -> CmdResult<photon_core::UsagesResult> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let impls = engine.index.implementations_of(&name).unwrap_or_default();
    let mut hits = Vec::new();
    for src in &impls {
        if let Some(sym) = engine
            .index
            .find_symbol(src)
            .unwrap_or_default()
            .into_iter()
            .find(|s| {
                matches!(
                    s.kind,
                    photon_core::SymbolKind::Class
                        | photon_core::SymbolKind::Interface
                        | photon_core::SymbolKind::Trait
                        | photon_core::SymbolKind::Enum
                )
            })
        {
            hits.push(photon_core::UsageHit {
                file: sym.file,
                line: sym.line,
                kind: "impl".into(),
                preview: sym.fqn.unwrap_or(sym.name),
                container: None,
            });
        }
    }
    Ok(photon_core::UsagesResult {
        title: format!("Implementations of {}", name),
        total: hits.len() as u32,
        hits,
    })
}

#[tauri::command]
fn usages_popup(name: String, state: State<'_, AppState>) -> CmdResult<photon_core::UsagesResult> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let refs = engine.index.references_to(&name).map_err(err)?;
    let def = engine.index.find_symbol(&name).ok().and_then(|d| d.into_iter().next());
    let title = match &def {
        Some(d) => format!(
            "{} {}",
            cap(d.kind.as_str()),
            d.fqn.clone().unwrap_or_else(|| d.name.clone())
        ),
        None => name.clone(),
    };
    // Symbol-resolved scoping: for a method/property, drop references whose
    // receiver resolves to an *unrelated* known class (keeps `$this`, `self`,
    // `$var` of a related type, and anything ambiguous). Class/function symbols
    // stay name-based (names are unique enough).
    let related: Option<std::collections::HashSet<String>> = def
        .as_ref()
        .filter(|d| matches!(d.kind, photon_core::SymbolKind::Method | photon_core::SymbolKind::Property))
        .and_then(|d| d.container.clone())
        .map(|scope| build_related(engine, &scope));

    let mut hits = Vec::new();
    let mut kept = 0u32;
    let mut text: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut syms: std::collections::HashMap<String, Vec<Symbol>> = std::collections::HashMap::new();
    for r in refs.into_iter() {
        if let Some(rel) = &related {
            let src = text
                .entry(r.file.clone())
                .or_insert_with(|| engine.workspace.read_file(&r.file).unwrap_or_default());
            if !ref_in_scope(engine, &r, rel, src, &mut syms) {
                continue;
            }
        }
        kept += 1;
        if hits.len() >= 80 {
            continue;
        }
        let src = text
            .entry(r.file.clone())
            .or_insert_with(|| engine.workspace.read_file(&r.file).unwrap_or_default());
        let preview = src
            .split('\n')
            .nth(r.line.saturating_sub(1) as usize)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        hits.push(photon_core::UsageHit {
            file: r.file,
            line: r.line,
            kind: r.kind.as_str().to_string(),
            preview,
            container: None,
        });
    }
    Ok(photon_core::UsagesResult { title, total: kept, hits })
}

/// `scope` plus its ancestors and descendants (for receiver-scoped usages).
fn build_related(engine: &photon_core::Engine, scope: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    set.insert(scope.to_string());
    let mut up = engine.index.supertypes(scope).unwrap_or_default();
    let mut budget = 0;
    while let Some(c) = up.pop() {
        if set.insert(c.clone()) && budget < 48 {
            budget += 1;
            up.extend(engine.index.supertypes(&c).unwrap_or_default());
        }
    }
    let mut down = engine.index.implementations_of(scope).unwrap_or_default();
    budget = 0;
    while let Some(c) = down.pop() {
        if set.insert(c.clone()) && budget < 96 {
            budget += 1;
            down.extend(engine.index.implementations_of(&c).unwrap_or_default());
        }
    }
    set
}

/// Receiver token immediately before a `->`/`::` member at `member_start`.
fn receiver_token(src: &str, member_start: usize) -> Option<String> {
    if member_start < 2 || member_start > src.len() {
        return None;
    }
    let pre = &src[..member_start];
    if !(pre.ends_with("->") || pre.ends_with("::")) {
        return None;
    }
    let rec_end = member_start - 2;
    let bytes = src.as_bytes();
    let mut start = rec_end;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'\\' {
            start -= 1;
        } else {
            break;
        }
    }
    Some(src[start..rec_end].to_string())
}

/// Keep a reference if its receiver is `$this`/`self`/`$var`/`Class` resolving
/// into the related-class set, or if the receiver is ambiguous/unknown.
fn ref_in_scope(
    engine: &photon_core::Engine,
    r: &Reference,
    related: &std::collections::HashSet<String>,
    src: &str,
    sym_cache: &mut std::collections::HashMap<String, Vec<Symbol>>,
) -> bool {
    let token = match receiver_token(src, r.start as usize) {
        Some(t) => t,
        None => return true,
    };
    if token.is_empty() {
        return true; // complex chain (`foo()->bar`) → keep
    }
    match token.as_str() {
        "$this" | "self" | "static" | "parent" => {
            let syms = sym_cache
                .entry(r.file.clone())
                .or_insert_with(|| engine.index.symbols_in_file(&r.file).unwrap_or_default());
            match photon_core::infer::enclosing_class(syms.as_slice(), r.start) {
                Some(c) => related.contains(&c),
                None => true,
            }
        }
        t if t.starts_with('$') => match photon_core::infer::infer_var_type(src, t) {
            Some(c) => related.contains(&c),
            None => true,
        },
        t => {
            let short = t.trim_start_matches('\\').rsplit('\\').next().unwrap_or(t);
            let known = engine
                .index
                .find_symbol(short)
                .map(|v| {
                    v.iter().any(|s| {
                        matches!(
                            s.kind,
                            photon_core::SymbolKind::Class
                                | photon_core::SymbolKind::Interface
                                | photon_core::SymbolKind::Trait
                                | photon_core::SymbolKind::Enum
                        )
                    })
                })
                .unwrap_or(false);
            if known {
                related.contains(short)
            } else {
                true
            }
        }
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

// ------------------------- Laravel depth -------------------------

#[tauri::command]
fn list_models(state: State<'_, AppState>) -> CmdResult<Vec<ModelInfo>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.models().map_err(err)
}

#[tauri::command]
fn config_key(key: String, state: State<'_, AppState>) -> CmdResult<Option<KeyEntry>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.config_key(&key).map_err(err)
}

#[tauri::command]
fn translation(key: String, state: State<'_, AppState>) -> CmdResult<Vec<KeyEntry>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.translation(&key).map_err(err)
}

#[tauri::command]
fn missing_translations(state: State<'_, AppState>) -> CmdResult<Vec<MissingTranslation>> {
    let guard = state.engine.lock();
    guard
        .as_ref()
        .ok_or("No project open")?
        .index
        .missing_translations()
        .map_err(err)
}

/// Resolve a Laravel string key (`config`/`route`/`trans`) to its definition.
#[tauri::command]
fn goto_laravel_key(
    kind: String,
    key: String,
    state: State<'_, AppState>,
) -> CmdResult<Option<photon_core::Location>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let loc = match kind.as_str() {
        "config" => engine
            .index
            .config_key(&key)
            .ok()
            .flatten()
            .map(|k| photon_core::Location { file: k.file, line: k.line }),
        "trans" => engine
            .index
            .translation(&key)
            .ok()
            .and_then(|v| v.into_iter().next())
            .map(|k| photon_core::Location { file: k.file, line: k.line }),
        "route" => engine
            .index
            .routes()
            .ok()
            .and_then(|rs| rs.into_iter().find(|r| r.name.as_deref() == Some(key.as_str())))
            .map(|r| photon_core::Location { file: r.file, line: r.line }),
        "env" => engine.workspace.roots.first().and_then(|root| {
            let txt = std::fs::read_to_string(root.path.join(".env")).ok()?;
            let line = txt
                .lines()
                .position(|l| {
                    let t = l.trim_start();
                    t.starts_with(&format!("{key}=")) || t.starts_with(&format!("{key} ="))
                })
                .map(|i| i as u32 + 1)?;
            Some(photon_core::Location {
                file: format!("{}/.env", root.label),
                line,
            })
        }),
        _ => None,
    };
    Ok(loc)
}

/// Navigate from a bound abstract (`app(Foo::class)`) to its concrete binding.
#[tauri::command]
fn goto_binding(name: String, state: State<'_, AppState>) -> CmdResult<Option<photon_core::Location>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let binds = engine.index.bindings().unwrap_or_default();
    let b = binds.into_iter().find(|b| {
        b.abstract_name == name || b.abstract_name.ends_with(&format!("\\{}", name))
    });
    if let Some(b) = b {
        if let Some(concrete) = b.concrete.filter(|c| c != "Closure") {
            let short = concrete.rsplit('\\').next().unwrap_or(&concrete).to_string();
            if let Some(sym) = engine
                .index
                .find_symbol(&short)
                .unwrap_or_default()
                .into_iter()
                .find(|s| matches!(s.kind, photon_core::SymbolKind::Class))
            {
                return Ok(Some(photon_core::Location { file: sym.file, line: sym.line }));
            }
        }
    }
    Ok(None)
}

#[tauri::command]
fn list_bindings(state: State<'_, AppState>) -> CmdResult<Vec<photon_core::Binding>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.bindings().map_err(err)
}

#[tauri::command]
fn list_events(state: State<'_, AppState>) -> CmdResult<Vec<photon_core::EventListener>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.events().map_err(err)
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> CmdResult<Vec<photon_core::JobInfo>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.jobs().map_err(err)
}

#[tauri::command]
fn list_artifacts(state: State<'_, AppState>) -> CmdResult<Vec<photon_core::ArtifactInfo>> {
    let guard = state.engine.lock();
    guard.as_ref().ok_or("No project open")?.index.artifacts().map_err(err)
}

// ------------------------- diagnostics & completion -------------------------

#[tauri::command]
fn lint_file(path: String, state: State<'_, AppState>) -> CmdResult<Vec<photon_core::Diagnostic>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    if !path.ends_with(".php") {
        return Ok(vec![]);
    }
    let src = match engine.workspace.read_file(&path) {
        Ok(s) => s,
        Err(_) => return Ok(vec![]),
    };
    let routes = engine.index.routes().unwrap_or_default();
    // File-local inspections (unused/duplicate imports, leftover debug calls).
    let mut out = photon_core::inspect::inspect_file(&src);
    for (kind, key, off) in photon_core::laravel::key_usages(&src) {
        let seg = key.split('.').next().unwrap_or(&key);
        let exists = match kind.as_str() {
            "route" => routes.iter().any(|r| r.name.as_deref() == Some(key.as_str())),
            // Lenient: if the config file (top-level namespace) is known, accept
            // deeper keys — they're often dynamic/data-driven.
            "config" => {
                engine.index.config_key(&key).ok().flatten().is_some()
                    || engine.index.config_namespace_known(seg).unwrap_or(true)
            }
            "trans" => {
                engine.index.translation(&key).map(|v| !v.is_empty()).unwrap_or(false)
                    || engine.index.translation_namespace_known(seg).unwrap_or(true)
            }
            _ => true, // views not indexed yet → don't flag
        };
        if exists {
            continue;
        }
        // byte offset → line/col (1-based line, 1-based col)
        let before = &src[..off];
        let line = before.matches('\n').count() as u32 + 1;
        let col = (off - before.rfind('\n').map(|i| i + 1).unwrap_or(0)) as u32 + 1;
        out.push(photon_core::Diagnostic {
            line,
            col,
            end_col: col + key.chars().count() as u32,
            message: format!("Unknown {} key '{}'", kind, key),
            severity: "warning".into(),
        });
    }

    // Type-based: undefined `$this->member` for confidently-typed classes.
    out.extend(undefined_this_members(engine, &path, &src));
    // #[Override] that doesn't actually override a parent method (PHP 8.3).
    out.extend(invalid_overrides(engine, &path, &src));

    // Unresolved-class diagnostics → drives the "import class" quick-fix.
    let imports = parse_imports(&src);
    let file_ns = parse_namespace(&src);
    let local: std::collections::HashSet<String> = engine
        .index
        .symbols_in_file(&path)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            matches!(
                s.kind,
                photon_core::SymbolKind::Class
                    | photon_core::SymbolKind::Interface
                    | photon_core::SymbolKind::Trait
                    | photon_core::SymbolKind::Enum
            )
        })
        .map(|s| s.name)
        .collect();

    let mut flagged = std::collections::HashSet::new();
    for r in photon_core::php::extract_references(&path, &src) {
        if !matches!(r.kind, photon_core::RefKind::TypeRef | photon_core::RefKind::StaticRef) {
            continue;
        }
        let name = &r.name;
        if imports.contains(name) || local.contains(name) || flagged.contains(name) {
            continue;
        }
        // Must be importable: a class-like symbol exists somewhere, in a
        // *different* namespace than this file's.
        let candidates = engine.index.find_symbol(name).unwrap_or_default();
        let file_ns_str = file_ns.clone().unwrap_or_default();
        let importable = candidates.iter().any(|c| {
            matches!(
                c.kind,
                photon_core::SymbolKind::Class
                    | photon_core::SymbolKind::Interface
                    | photon_core::SymbolKind::Trait
                    | photon_core::SymbolKind::Enum
            ) && c
                .fqn
                .as_deref()
                .map(|f| {
                    let ns = namespace_of(f);
                    !ns.is_empty() && ns != file_ns_str
                })
                .unwrap_or(false)
        });
        if !importable {
            continue;
        }
        flagged.insert(name.clone());
        out.push(photon_core::Diagnostic {
            line: r.line,
            col: r.column,
            end_col: r.column + name.chars().count() as u32,
            message: format!("Class '{}' is not imported", name),
            severity: "warning".into(),
        });
    }
    Ok(out)
}

/// Short names imported via `use ...;` (handles `as` aliases).
fn parse_imports(src: &str) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for line in src.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("use ") {
            let rest = rest.trim_end_matches(';').trim();
            // skip group/function/const uses for v1 simplicity
            if rest.contains('{') || rest.starts_with("function ") || rest.starts_with("const ") {
                continue;
            }
            let short = if let Some(idx) = rest.find(" as ") {
                rest[idx + 4..].trim().to_string()
            } else {
                rest.rsplit('\\').next().unwrap_or(rest).to_string()
            };
            if !short.is_empty() {
                set.insert(short);
            }
        }
    }
    set
}

fn parse_namespace(src: &str) -> Option<String> {
    for line in src.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("namespace ") {
            return Some(rest.trim_end_matches(';').trim().to_string());
        }
    }
    None
}

fn namespace_of(fqn: &str) -> String {
    match fqn.trim_start_matches('\\').rfind('\\') {
        Some(i) => fqn.trim_start_matches('\\')[..i].to_string(),
        None => String::new(),
    }
}

/// Full member-name set of a class (own + extends chain + traits). Returns
/// `None` when the inspection must be skipped for safety: the class (or an
/// ancestor) uses magic (`__call`/`__get`/…), is an Eloquent model (dynamic
/// columns), or has an unresolvable ancestor (e.g. a PHP builtin we don't index).
fn class_member_set(
    engine: &photon_core::Engine,
    root: &str,
    models: &std::collections::HashSet<String>,
) -> Option<std::collections::HashSet<String>> {
    use std::collections::HashSet;
    let mut set: HashSet<String> = HashSet::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue = vec![root.to_string()];
    let mut budget = 0;
    while let Some(c) = queue.pop() {
        if !visited.insert(c.clone()) {
            continue;
        }
        budget += 1;
        if budget > 12 {
            break;
        }
        if models.contains(&c) {
            return None; // Eloquent magic columns
        }
        let members = engine.index.members_of(&c).unwrap_or_default();
        let has_symbol = engine
            .index
            .find_symbol(&c)
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        // An ancestor we can neither resolve nor see members of → unknown base.
        if c != root && !has_symbol && members.is_empty() {
            return None;
        }
        for m in &members {
            if matches!(m.name.as_str(), "__call" | "__get" | "__callStatic" | "__set" | "__isset") {
                return None;
            }
            set.insert(m.name.clone());
        }
        for sup in engine.index.supertypes(&c).unwrap_or_default() {
            queue.push(sup);
        }
    }
    Some(set)
}

/// Names of constructor-promoted properties (`__construct(private Foo $x)`).
fn promoted_properties(region: &str) -> Vec<String> {
    let mut out = Vec::new();
    let p = match region.find("__construct") {
        Some(p) => p,
        None => return out,
    };
    let open = match region[p..].find('(') {
        Some(o) => p + o,
        None => return out,
    };
    let mut depth = 0i32;
    let mut params = String::new();
    for ch in region[open..].chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        if depth >= 1 && ch != '(' {
            params.push(ch);
        }
    }
    for part in params.split(',') {
        let pl = part.trim();
        let promoted = ["public", "private", "protected", "readonly"]
            .iter()
            .any(|m| pl.starts_with(m));
        if promoted {
            if let Some(d) = pl.find('$') {
                let name: String = pl[d + 1..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// Flag `$receiver->member` accesses that aren't defined on the (confidently
/// known) receiver class — for `$this` and for any `$var` whose type the engine
/// can resolve to a first-party class. Conservative: only the simple
/// `$var->member` form (not chains), and skips magic/model/unknown-ancestor
/// classes, so it never fires when the type is uncertain.
fn undefined_this_members(
    engine: &photon_core::Engine,
    path: &str,
    src: &str,
) -> Vec<photon_core::Diagnostic> {
    use std::collections::{HashMap, HashSet};
    let mut out = Vec::new();
    if path.contains("/vendor/") {
        return out;
    }
    let models: HashSet<String> =
        engine.index.model_names().unwrap_or_default().into_iter().collect();
    let symbols = engine.index.symbols_in_file(path).unwrap_or_default();

    // Cache resolved member sets per class (None = unsafe to inspect) and
    // resolved types per `$var` so a large file stays cheap.
    let mut sets: HashMap<String, Option<HashSet<String>>> = HashMap::new();
    let mut var_types: HashMap<String, Option<String>> = HashMap::new();

    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < src.len() {
        let rel = match src[i..].find("->") {
            Some(p) => p,
            None => break,
        };
        let arrow = i + rel;
        i = arrow + 2;
        // Receiver must be a bare `$ident` immediately before `->` (no chains,
        // no `)`/`]` — those need full chain resolution we don't attempt here).
        let recv_end = arrow;
        let mut s = recv_end;
        while s > 0 {
            let c = bytes[s - 1];
            if c.is_ascii_alphanumeric() || c == b'_' {
                s -= 1;
            } else {
                break;
            }
        }
        if s == recv_end || s == 0 || bytes[s - 1] != b'$' {
            continue;
        }
        let var = &src[s - 1..recv_end]; // includes leading `$`
        let mat = arrow + 2;
        match bytes.get(mat).copied() {
            Some(b'{') | Some(b'$') => continue, // dynamic access
            _ => {}
        }
        let member: String = src[mat..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if member.is_empty() {
            continue;
        }

        // Resolve the receiver to a class name.
        let class = if var == "$this" {
            photon_core::infer::enclosing_class(&symbols, (s - 1) as u32)
        } else {
            var_types
                .entry(var.to_string())
                .or_insert_with(|| photon_core::infer::infer_var_type(src, var))
                .clone()
        };
        let class = match class {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };

        if !sets.contains_key(&class) {
            let mut set = class_member_set(engine, &class, &models);
            // Fold in constructor-promoted properties when the class is defined
            // in this file (covers brand-new code before a reindex lands).
            if let Some(ref mut members) = set {
                if let Some(csym) = symbols.iter().find(|sy| {
                    sy.name == class
                        && matches!(sy.kind, photon_core::SymbolKind::Class)
                        && sy.range_end > sy.range_start
                }) {
                    let st = (csym.range_start as usize).min(src.len());
                    let en = (csym.range_end as usize).min(src.len());
                    for p in promoted_properties(&src[st..en]) {
                        members.insert(p);
                    }
                }
            }
            sets.insert(class.clone(), set);
        }
        let set = match sets.get(&class).and_then(|o| o.as_ref()) {
            Some(s) => s,
            None => continue, // unsafe to inspect this class
        };
        if set.contains(&member) {
            continue;
        }

        // Point the diagnostic at the member name.
        let before = &src[..mat];
        let line = before.matches('\n').count() as u32 + 1;
        let col = (mat - before.rfind('\n').map(|x| x + 1).unwrap_or(0)) as u32 + 1;
        let is_call = src[mat + member.len()..].trim_start().starts_with('(');
        out.push(photon_core::Diagnostic {
            line,
            col,
            end_col: col + member.chars().count() as u32,
            message: format!(
                "Undefined {} '{}->{}' on {}",
                if is_call { "method" } else { "property" },
                var,
                member,
                class
            ),
            severity: "warning".into(),
        });
    }
    out
}

/// Flag methods marked `#[Override]` that don't actually override an ancestor
/// method (PHP 8.3). Cross-file: walks the supertype chain via the index.
fn invalid_overrides(
    engine: &photon_core::Engine,
    path: &str,
    src: &str,
) -> Vec<photon_core::Diagnostic> {
    use std::collections::HashSet;
    let mut out = Vec::new();
    if path.contains("/vendor/") {
        return out;
    }
    let symbols = engine.index.symbols_in_file(path).unwrap_or_default();
    for s in symbols.iter().filter(|s| matches!(s.kind, photon_core::SymbolKind::Method)) {
        let container = match &s.container {
            Some(c) => c,
            None => continue,
        };
        // Is the method annotated with #[Override]? Scan a small window before it.
        let start = s.range_start as usize;
        let win = &src[start.saturating_sub(200)..start.min(src.len())];
        let annotated = win.contains("#[Override")
            || win.contains("#[\\Override")
            || win.contains("#[ Override");
        if !annotated {
            continue;
        }
        // Does any ancestor declare a method of the same name?
        let mut found = false;
        let mut queue = engine.index.supertypes(container).unwrap_or_default();
        let mut visited: HashSet<String> = HashSet::new();
        let mut budget = 0;
        while let Some(c) = queue.pop() {
            if !visited.insert(c.clone()) || budget > 16 {
                continue;
            }
            budget += 1;
            if engine
                .index
                .members_of(&c)
                .unwrap_or_default()
                .iter()
                .any(|m| m.name == s.name && matches!(m.kind, photon_core::SymbolKind::Method))
            {
                found = true;
                break;
            }
            for sup in engine.index.supertypes(&c).unwrap_or_default() {
                queue.push(sup);
            }
        }
        if found {
            continue;
        }
        let abs = s.name_offset as usize;
        let before = &src[..abs.min(src.len())];
        let line = before.matches('\n').count() as u32 + 1;
        let col = (abs - before.rfind('\n').map(|x| x + 1).unwrap_or(0)) as u32 + 1;
        out.push(photon_core::Diagnostic {
            line,
            col,
            end_col: col + s.name.chars().count() as u32,
            message: format!(
                "'{}' has #[Override] but does not override a parent method",
                s.name
            ),
            severity: "error".into(),
        });
    }
    out
}

/// Parameter names for a callee (function, method, or a class's constructor),
/// powering named-argument completion.
#[tauri::command]
fn call_params(name: String, state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let short = name.rsplit('\\').next().unwrap_or(&name).to_string();
    let defs = engine.index.find_symbol(&short).unwrap_or_default();
    let params_from = |file: &str, start: u32, end: u32| -> Vec<String> {
        let src = engine.workspace.read_file(file).unwrap_or_default();
        let (s, e) = ((start as usize).min(src.len()), (end as usize).min(src.len()));
        if e > s {
            photon_core::php::param_names(&src[s..e])
        } else {
            Vec::new()
        }
    };
    for d in &defs {
        match d.kind {
            photon_core::SymbolKind::Method | photon_core::SymbolKind::Function => {
                let p = params_from(&d.file, d.range_start, d.range_end);
                if !p.is_empty() {
                    return Ok(p);
                }
            }
            photon_core::SymbolKind::Class
            | photon_core::SymbolKind::Interface
            | photon_core::SymbolKind::Trait => {
                if let Some(ctor) = engine
                    .index
                    .members_of(&d.name)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|m| m.name == "__construct")
                {
                    let p = params_from(&ctor.file, ctor.range_start, ctor.range_end);
                    if !p.is_empty() {
                        return Ok(p);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(Vec::new())
}

#[derive(serde::Serialize)]
struct SymbolDoc {
    name: String,
    kind: String,
    signature: String,
    params: Vec<(String, String)>,
    return_type: String,
    doc: String,
    source: String,
}

/// Rich documentation for a symbol (PhpStorm-style hover popup): signature,
/// parameters, return type, description, and source path.
#[tauri::command]
fn symbol_doc(name: String, state: State<'_, AppState>) -> CmdResult<Option<SymbolDoc>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let short = name.rsplit('\\').next().unwrap_or(&name).to_string();
    let def = engine
        .index
        .find_symbol(&short)
        .unwrap_or_default()
        .into_iter()
        .find(|d| {
            matches!(
                d.kind,
                photon_core::SymbolKind::Method
                    | photon_core::SymbolKind::Function
                    | photon_core::SymbolKind::Class
                    | photon_core::SymbolKind::Interface
                    | photon_core::SymbolKind::Trait
                    | photon_core::SymbolKind::Enum
            )
        });
    let d = match def {
        Some(d) => d,
        None => return Ok(None),
    };
    let src = engine.workspace.read_file(&d.file).unwrap_or_default();
    let (s, e) = ((d.range_start as usize).min(src.len()), (d.range_end as usize).min(src.len()));
    let decl = if e > s { &src[s..e] } else { "" };
    let header = decl.split('{').next().unwrap_or(decl);
    let signature = header.split_whitespace().collect::<Vec<_>>().join(" ");
    let doc = photon_core::php::doc_before(&src, s);
    let is_callable = matches!(d.kind, photon_core::SymbolKind::Method | photon_core::SymbolKind::Function);
    let params = if is_callable { photon_core::php::param_specs(decl) } else { Vec::new() };
    let return_type = doc
        .and_then(photon_core::phpdoc::raw_return)
        .or_else(|| photon_core::php::return_type(decl))
        .unwrap_or_default();
    let description = doc.and_then(photon_core::phpdoc::description).unwrap_or_default();
    Ok(Some(SymbolDoc {
        kind: d.kind.as_str().to_string(),
        signature,
        params,
        return_type,
        doc: description,
        source: d.file,
        name: d.name,
    }))
}

#[derive(serde::Serialize)]
struct ReturnFix {
    ty: String,
    line: u32,
    col: u32,
}

/// Suggest a return type + insertion position for the method/function whose
/// name is on `line` (drives the "Add return type" quick-fix).
#[tauri::command]
fn return_type_fix(path: String, line: u32, state: State<'_, AppState>) -> CmdResult<Option<ReturnFix>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let src = engine.workspace.read_file(&path).map_err(err)?;
    let s = match photon_core::php::analyze_return(&src, line) {
        Some(s) => s,
        None => return Ok(None),
    };
    // Confident literal/doc type wins; otherwise resolve the return expression
    // through the type engine. If neither resolves, offer no fix (never `mixed`).
    let ty = s.literal.or_else(|| {
        s.chain
            .and_then(|(chain, off)| resolve_chain_class(engine, &path, off, &chain))
    });
    Ok(ty.map(|ty| ReturnFix { ty, line: s.insert_line, col: s.insert_col }))
}

/// Blade view names (dotted, e.g. `admin.users.index`) discovered under
/// `resources/views/` — completion + nav for `view()`, `@extends`, `@include`.
#[tauri::command]
fn blade_views(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let mut out: Vec<String> = Vec::new();
    for fe in engine.workspace.list_files() {
        if !fe.path.ends_with(".blade.php") {
            continue;
        }
        if let Some(idx) = fe.path.find("resources/views/") {
            let rel = &fe.path[idx + "resources/views/".len()..];
            let name = rel.trim_end_matches(".blade.php").replace('/', ".");
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Tables + columns from migrations, for schema-aware SQL completion inside
/// PHP string literals.
#[tauri::command]
fn schema_tables(state: State<'_, AppState>) -> CmdResult<Vec<(String, Vec<String>)>> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    engine.index.tables_with_columns().map_err(err)
}

#[tauri::command]
fn completion_data(state: State<'_, AppState>) -> CmdResult<photon_core::CompletionData> {
    let guard = state.engine.lock();
    let engine = guard.as_ref().ok_or("No project open")?;
    let routes = engine.index.routes().unwrap_or_default();
    let configs = engine.index.config_key_candidates("", 2000).unwrap_or_default();
    let classes: Vec<String> = engine
        .index
        .symbol_candidates("", 3000)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| {
            matches!(
                s.kind,
                photon_core::SymbolKind::Class
                    | photon_core::SymbolKind::Interface
                    | photon_core::SymbolKind::Trait
                    | photon_core::SymbolKind::Enum
            )
        })
        .map(|s| s.name)
        .collect();
    // distinct translation keys
    let mut translations: Vec<String> = Vec::new();
    if let Ok(missing) = engine.index.missing_translations() {
        for m in missing {
            translations.push(m.key);
        }
    }
    // .env keys from the primary project (env() completion — Laravel Idea style).
    let mut envs: Vec<String> = Vec::new();
    if let Some(root) = engine.workspace.primary_path() {
        for fname in [".env", ".env.example"] {
            if let Ok(txt) = std::fs::read_to_string(root.join(fname)) {
                for line in txt.lines() {
                    let l = line.trim();
                    if l.starts_with('#') || !l.contains('=') {
                        continue;
                    }
                    if let Some(k) = l.split('=').next() {
                        let k = k.trim().to_string();
                        if !k.is_empty() && !envs.contains(&k) {
                            envs.push(k);
                        }
                    }
                }
                break;
            }
        }
    }

    // Middleware aliases (defaults + Kernel/bootstrap) and FormRequest input keys.
    let mut middlewares: Vec<String> = [
        "auth", "guest", "throttle", "verified", "signed", "can", "auth.basic",
        "password.confirm", "cache.headers", "precognitive",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(root) = engine.workspace.primary_path() {
        for f in ["app/Http/Kernel.php", "bootstrap/app.php"] {
            if let Ok(txt) = std::fs::read_to_string(root.join(f)) {
                for m in photon_core::laravel::parse_middleware_aliases(&txt) {
                    if !middlewares.contains(&m) {
                        middlewares.push(m);
                    }
                }
            }
        }
    }
    let mut request_keys: Vec<String> = Vec::new();
    for fe in engine.workspace.list_files() {
        if fe.path.contains("app/Http/Requests") && fe.path.ends_with(".php") {
            if let Ok(src) = engine.workspace.read_file(&fe.path) {
                for k in photon_core::laravel::rules_keys(&src) {
                    if !request_keys.contains(&k) {
                        request_keys.push(k);
                    }
                }
            }
        }
    }

    Ok(photon_core::CompletionData {
        routes: routes.into_iter().filter_map(|r| r.name).collect(),
        configs: configs.into_iter().map(|c| c.key).collect(),
        translations,
        classes,
        envs,
        middlewares,
        request_keys,
    })
}

// ------------------------- database tools (async) -------------------------

#[tauri::command]
async fn db_connect(name: String, url: String, db: State<'_, DbManager>) -> CmdResult<String> {
    db.connect(&name, &url).await
}

// ------------------------- Redis console (NoSQL) -------------------------

#[tauri::command]
fn redis_connect(name: String, url: String, r: State<'_, redis_client::RedisManager>) -> CmdResult<String> {
    r.connect(&name, &url)
}

#[tauri::command]
fn redis_disconnect(name: String, r: State<'_, redis_client::RedisManager>) -> CmdResult<()> {
    r.disconnect(&name);
    Ok(())
}

#[tauri::command]
fn redis_connections(r: State<'_, redis_client::RedisManager>) -> CmdResult<Vec<String>> {
    Ok(r.connections())
}

#[tauri::command]
fn redis_command(
    name: String,
    parts: Vec<String>,
    r: State<'_, redis_client::RedisManager>,
) -> CmdResult<String> {
    r.command(&name, &parts)
}

// ------------------------- Xdebug (DBGp debugger) -------------------------

#[tauri::command]
fn debug_listen(app: AppHandle, dbg: State<'_, debugger::DebugState>) -> CmdResult<()> {
    debugger::listen(app, &dbg)
}

/// Continuation/stop control: verb ∈ run | step_into | step_over | step_out | stop.
#[tauri::command]
fn debug_command(verb: String, dbg: State<'_, debugger::DebugState>) -> CmdResult<()> {
    if let Some(tx) = dbg.tx.lock().unwrap().as_ref() {
        let _ = tx.send(format!("{verb}\t"));
    }
    if verb == "stop" {
        *dbg.listening.lock().unwrap() = false;
        *dbg.tx.lock().unwrap() = None;
    }
    Ok(())
}

#[tauri::command]
fn debug_set_breakpoint(
    path: String,
    line: u32,
    condition: Option<String>,
    state: State<'_, AppState>,
    dbg: State<'_, debugger::DebugState>,
) -> CmdResult<()> {
    let abs = {
        let guard = state.engine.lock();
        guard
            .as_ref()
            .and_then(|e| e.workspace.abs_path(&path))
            .map(|p| p.to_string_lossy().to_string())
    };
    let abs = match abs {
        Some(a) => a,
        None => return Ok(()),
    };
    let cond = condition.filter(|c| !c.trim().is_empty());
    dbg.breakpoints.lock().unwrap().push((abs.clone(), line, cond.clone()));
    if let Some(tx) = dbg.tx.lock().unwrap().as_ref() {
        let args = debugger::breakpoint_args(&abs, line, cond.as_deref());
        let _ = tx.send(format!("breakpoint_set\t{args}"));
    }
    Ok(())
}

#[tauri::command]
fn debug_remove_breakpoint(
    path: String,
    line: u32,
    state: State<'_, AppState>,
    dbg: State<'_, debugger::DebugState>,
) -> CmdResult<()> {
    let abs = {
        let guard = state.engine.lock();
        guard
            .as_ref()
            .and_then(|e| e.workspace.abs_path(&path))
            .map(|p| p.to_string_lossy().to_string())
    };
    if let Some(abs) = abs {
        dbg.breakpoints.lock().unwrap().retain(|(f, l, _)| !(f == &abs && *l == line));
    }
    Ok(())
}

/// Expand a variable's children in the debugger (sends `property_get`; the
/// result arrives via the `xdebug-property` event).
#[tauri::command]
fn debug_property(name: String, dbg: State<'_, debugger::DebugState>) -> CmdResult<()> {
    if let Some(tx) = dbg.tx.lock().unwrap().as_ref() {
        let _ = tx.send(format!("property_get\t-n {name}"));
    }
    Ok(())
}

/// Map an absolute path (from a break event) back to a workspace path so the UI
/// can open it.
#[tauri::command]
fn path_to_workspace(abs: String, state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let guard = state.engine.lock();
    Ok(guard.as_ref().and_then(|e| e.workspace.wpath_of_abs(&abs)))
}

#[tauri::command]
async fn db_disconnect(name: String, db: State<'_, DbManager>) -> CmdResult<()> {
    db.disconnect(&name).await;
    Ok(())
}

#[tauri::command]
async fn db_connections(db: State<'_, DbManager>) -> CmdResult<Vec<String>> {
    Ok(db.connections().await)
}

#[tauri::command]
async fn db_schema(name: String, db: State<'_, DbManager>) -> CmdResult<DbSchema> {
    db.schema(&name).await
}

#[tauri::command]
async fn db_query(name: String, sql: String, db: State<'_, DbManager>) -> CmdResult<QueryResult> {
    db.query(&name, &sql).await
}

#[tauri::command]
async fn db_update_cell(
    name: String,
    table: String,
    column: String,
    value: String,
    pk_column: String,
    pk_value: String,
    db: State<'_, DbManager>,
) -> CmdResult<u64> {
    db.update_cell(&name, &table, &column, &value, &pk_column, &pk_value)
        .await
}

// ------------------------- git -------------------------

#[tauri::command]
fn git_is_repo(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(git::is_repo(&project_root(&state)?))
}

#[tauri::command]
fn git_status(state: State<'_, AppState>) -> CmdResult<git::GitStatus> {
    git::status(&project_root(&state)?)
}

#[tauri::command]
fn git_stage(paths: Vec<String>, state: State<'_, AppState>) -> CmdResult<()> {
    git::stage(&project_root(&state)?, &paths)
}

#[tauri::command]
fn git_unstage(paths: Vec<String>, state: State<'_, AppState>) -> CmdResult<()> {
    git::unstage(&project_root(&state)?, &paths)
}

#[tauri::command]
fn git_commit(message: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::commit(&project_root(&state)?, &message)
}

#[tauri::command]
fn git_branches(state: State<'_, AppState>) -> CmdResult<Vec<git::Branch>> {
    git::branches(&project_root(&state)?)
}

#[tauri::command]
fn git_checkout(branch: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::checkout(&project_root(&state)?, &branch)
}

#[tauri::command]
fn git_create_branch(name: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::create_branch(&project_root(&state)?, &name)
}

#[tauri::command]
fn git_diff(file: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::diff(&project_root(&state)?, &file)
}

#[tauri::command]
fn git_log(limit: u32, state: State<'_, AppState>) -> CmdResult<Vec<git::GitCommit>> {
    git::log(&project_root(&state)?, limit)
}

#[tauri::command]
fn git_push(state: State<'_, AppState>) -> CmdResult<String> {
    git::push(&project_root(&state)?)
}

#[tauri::command]
fn git_pull(state: State<'_, AppState>) -> CmdResult<String> {
    git::pull(&project_root(&state)?)
}

#[tauri::command]
fn git_stash(state: State<'_, AppState>) -> CmdResult<String> {
    git::stash(&project_root(&state)?)
}

#[tauri::command]
fn git_stash_pop(state: State<'_, AppState>) -> CmdResult<String> {
    git::stash_pop(&project_root(&state)?)
}

#[tauri::command]
fn git_graph(limit: u32, state: State<'_, AppState>) -> CmdResult<Vec<git::GraphCommit>> {
    git::graph(&project_root(&state)?, limit)
}

#[tauri::command]
fn git_blame(file: String, state: State<'_, AppState>) -> CmdResult<Vec<git::BlameLine>> {
    git::blame(&project_root(&state)?, &file)
}

#[tauri::command]
fn git_diff_sides(file: String, state: State<'_, AppState>) -> CmdResult<git::DiffSides> {
    git::diff_sides(&project_root(&state)?, &file)
}

#[tauri::command]
fn git_cherry_pick(hash: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::cherry_pick(&project_root(&state)?, &hash)
}

#[tauri::command]
fn git_compare(base: String, head: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::compare(&project_root(&state)?, &base, &head)
}

#[tauri::command]
fn git_conflicts(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    git::conflicts(&project_root(&state)?)
}

#[tauri::command]
fn git_resolve(file: String, side: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::resolve_conflict(&project_root(&state)?, &file, &side)
}

#[tauri::command]
fn git_discard(file: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::discard(&project_root(&state)?, &file)
}

#[tauri::command]
fn git_suggest_message(state: State<'_, AppState>) -> CmdResult<String> {
    git::suggest_commit_message(&project_root(&state)?)
}

#[tauri::command]
fn git_amend(message: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::amend(&project_root(&state)?, &message)
}

#[tauri::command]
fn git_reset(target: String, mode: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::reset(&project_root(&state)?, &target, &mode)
}

#[tauri::command]
fn git_revert(hash: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::revert(&project_root(&state)?, &hash)
}

#[tauri::command]
fn git_file_diff(file: String, staged: bool, state: State<'_, AppState>) -> CmdResult<String> {
    git::file_diff(&project_root(&state)?, &file, staged)
}

#[tauri::command]
fn git_apply_hunk(patch: String, reverse: bool, state: State<'_, AppState>) -> CmdResult<String> {
    git::apply_hunk(&project_root(&state)?, &patch, reverse)
}

#[tauri::command]
fn git_conflict_versions(
    file: String,
    state: State<'_, AppState>,
) -> CmdResult<git::ConflictVersions> {
    git::conflict_versions(&project_root(&state)?, &file)
}

#[tauri::command]
fn git_resolve_content(file: String, content: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::resolve_content(&project_root(&state)?, &file, &content)
}

#[tauri::command]
fn git_merge(branch: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::merge(&project_root(&state)?, &branch)
}

#[tauri::command]
fn git_branch_force(name: String, target: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::branch_force(&project_root(&state)?, &name, &target)
}

#[tauri::command]
fn git_insights(state: State<'_, AppState>) -> CmdResult<git::Insights> {
    git::insights(&project_root(&state)?)
}

#[tauri::command]
fn git_line_status(file: String, state: State<'_, AppState>) -> CmdResult<git::LineStatus> {
    // `file` is a workspace path; git wants it relative to the repo root.
    let rel = file.split_once('/').map(|(_, r)| r.to_string()).unwrap_or(file);
    git::line_status(&project_root(&state)?, &rel)
}

#[tauri::command]
fn git_pr_url(state: State<'_, AppState>) -> CmdResult<String> {
    git::pr_url(&project_root(&state)?)
}

/// Open a URL in the user's default browser via the OS opener.
#[tauri::command]
fn open_external(url: String) -> CmdResult<()> {
    #[cfg(target_os = "macos")]
    let res = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let res = std::process::Command::new("xdg-open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let res = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let res: std::io::Result<std::process::Child> =
        Err(std::io::Error::new(std::io::ErrorKind::Other, "unsupported platform"));
    res.map(|_| ()).map_err(err)
}

#[tauri::command]
fn git_rebase_list(base: String, state: State<'_, AppState>) -> CmdResult<Vec<git::GitCommit>> {
    git::rebase_list(&project_root(&state)?, &base)
}

#[tauri::command]
fn git_rebase_interactive(base: String, todo: String, state: State<'_, AppState>) -> CmdResult<String> {
    git::rebase_interactive(&project_root(&state)?, &base, &todo)
}

#[tauri::command]
fn git_branches_detailed(state: State<'_, AppState>) -> CmdResult<Vec<git::BranchInfo>> {
    git::branches_detailed(&project_root(&state)?)
}

#[tauri::command]
fn git_update(state: State<'_, AppState>) -> CmdResult<String> {
    git::update_project(&project_root(&state)?)
}

// ------------------------- data sources (advanced DB) -------------------------

#[tauri::command]
fn db_list_sources(state: State<'_, AppState>) -> CmdResult<Vec<DataSource>> {
    let root = project_root(&state)?;
    Ok(dbtools::load_sources(&root)
        .iter()
        .map(DataSource::sanitized)
        .collect())
}

#[tauri::command]
fn db_save_source(source: DataSource, state: State<'_, AppState>) -> CmdResult<Vec<DataSource>> {
    dbtools::save_source(&project_root(&state)?, source)
}

#[tauri::command]
fn db_delete_source(id: String, state: State<'_, AppState>) -> CmdResult<Vec<DataSource>> {
    dbtools::delete_source(&project_root(&state)?, &id)
}

#[tauri::command]
async fn db_test_source(source: DataSource, password: Option<String>) -> CmdResult<String> {
    dbtools::test_url(&source.url(password.as_deref())).await
}

#[tauri::command]
async fn db_connect_source(
    id: String,
    password: Option<String>,
    state: State<'_, AppState>,
    db: State<'_, DbManager>,
) -> CmdResult<String> {
    // Resolve the full (non-sanitized) source so a saved password can be used.
    let (name, url) = {
        let root = project_root(&state)?;
        let src = dbtools::load_sources(&root)
            .into_iter()
            .find(|s| s.id == id)
            .ok_or("data source not found")?;
        (src.name.clone(), src.url(password.as_deref()))
    };
    db.connect(&name, &url).await
}

// ------------------------- system stats (status bar) -------------------------

#[derive(serde::Serialize)]
struct SystemStats {
    php_version: String,
    memory_mb: u64,
    indexed_files: u32,
}

fn detect_php_version(root: Option<&str>) -> String {
    // Prefer the runtime PHP; fall back to composer.json's php constraint.
    if let Ok(out) = std::process::Command::new("php")
        .args(["-r", "echo PHP_VERSION;"])
        .output()
    {
        if out.status.success() {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !v.is_empty() {
                return format!("PHP {v}");
            }
        }
    }
    if let Some(r) = root {
        if let Ok(txt) = std::fs::read_to_string(std::path::Path::new(r).join("composer.json")) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                if let Some(c) = v.get("require").and_then(|r| r.get("php")).and_then(|p| p.as_str()) {
                    return format!("PHP {c}");
                }
            }
        }
    }
    "PHP —".into()
}

#[tauri::command]
fn system_stats(state: State<'_, AppState>) -> CmdResult<SystemStats> {
    use sysinfo::{ProcessRefreshKind, System};
    let (root, indexed) = {
        let guard = state.engine.lock();
        match guard.as_ref() {
            Some(e) => (
                e.workspace.primary_path().map(|p| p.to_string_lossy().to_string()),
                e.index.count("files").unwrap_or(0),
            ),
            None => (None, 0),
        }
    };

    let mut sys = System::new();
    let mut memory_mb = 0u64;
    if let Ok(pid) = sysinfo::get_current_pid() {
        sys.refresh_process_specifics(pid, ProcessRefreshKind::new().with_memory());
        if let Some(p) = sys.process(pid) {
            // sysinfo 0.30 reports memory in bytes.
            memory_mb = p.memory() / 1024 / 1024;
        }
    }

    Ok(SystemStats {
        php_version: detect_php_version(root.as_deref()),
        memory_mb,
        indexed_files: indexed,
    })
}

// ------------------------- AI workspace (v2 W3) -------------------------

#[tauri::command]
async fn ai_chat(
    base_url: String,
    api_key: String,
    model: String,
    messages: Vec<ai::ChatMessage>,
    context: Option<String>,
) -> CmdResult<String> {
    let mut msgs: Vec<ai::ChatMessage> = Vec::new();
    let mut system = String::from(
        "You are Photon, an expert PHP & Laravel pair-programmer embedded in an IDE. \
         Be concise and produce correct, idiomatic Laravel code. Prefer fenced code blocks.",
    );
    if let Some(ctx) = context {
        if !ctx.trim().is_empty() {
            system.push_str("\n\n--- Project context ---\n");
            system.push_str(&ctx);
        }
    }
    msgs.push(ai::ChatMessage { role: "system".into(), content: system });
    msgs.extend(messages);
    ai::chat(&base_url, &api_key, &model, msgs).await
}

// ------------------------- HTTP API client (bottom dock) -------------------------

#[derive(serde::Serialize)]
struct HttpResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: String,
    duration_ms: u64,
    size: usize,
}

#[tauri::command]
async fn http_request(
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
) -> CmdResult<HttpResponse> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(err)?;
    let m = reqwest::Method::from_bytes(method.to_uppercase().as_bytes()).map_err(err)?;
    let mut req = client.request(m, &url);
    for (k, v) in &headers {
        if !k.trim().is_empty() {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    if let Some(b) = body {
        if !b.is_empty() {
            req = req.body(b);
        }
    }
    let started = std::time::Instant::now();
    let resp = req.send().await.map_err(err)?;
    let status = resp.status();
    let status_text = status.canonical_reason().unwrap_or("").to_string();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let text = resp.text().await.unwrap_or_default();
    let duration_ms = started.elapsed().as_millis() as u64;
    let size = text.len();
    Ok(HttpResponse {
        status: status.as_u16(),
        status_text,
        headers: resp_headers,
        body: text,
        duration_ms,
        size,
    })
}

// ------------------------- settings persistence -------------------------

fn settings_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

#[tauri::command]
fn settings_load(app: AppHandle) -> CmdResult<String> {
    match settings_file(&app) {
        Some(p) => Ok(std::fs::read_to_string(p).unwrap_or_default()),
        None => Ok(String::new()),
    }
}

#[tauri::command]
fn settings_save(app: AppHandle, json: String) -> CmdResult<()> {
    let path = settings_file(&app).ok_or("no config dir")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    std::fs::write(path, json).map_err(err)
}

// ------------------------- integrated terminal -------------------------

#[tauri::command]
fn term_spawn(
    app: AppHandle,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    terms: State<'_, Terminals>,
) -> CmdResult<String> {
    terms.spawn(app, cwd, cols, rows)
}

#[tauri::command]
fn term_write(id: String, data: String, terms: State<'_, Terminals>) -> CmdResult<()> {
    terms.write(&id, &data)
}

#[tauri::command]
fn term_resize(id: String, cols: u16, rows: u16, terms: State<'_, Terminals>) -> CmdResult<()> {
    terms.resize(&id, cols, rows)
}

#[tauri::command]
fn term_kill(id: String, terms: State<'_, Terminals>) -> CmdResult<()> {
    terms.kill(&id);
    Ok(())
}

// ------------------------- templates -------------------------

fn all_templates(root: &str) -> Vec<Template> {
    let mut all = templates::builtins();
    all.extend(templates::user_templates(root));
    all.extend(extensions::contributed_templates(root));
    all
}

#[tauri::command]
fn template_list(state: State<'_, AppState>) -> CmdResult<Vec<Template>> {
    Ok(all_templates(&project_root(&state)?))
}

#[tauri::command]
fn template_create(
    template_id: String,
    vars: HashMap<String, String>,
    state: State<'_, AppState>,
) -> CmdResult<String> {
    let mut guard = state.engine.lock();
    let engine = guard.as_mut().ok_or("No project open")?;
    // New files go into the primary (first) project root.
    let primary = engine.workspace.roots.first().ok_or("No project open")?;
    let label = primary.label.clone();
    let root = primary.path.to_string_lossy().to_string();
    let tpl = all_templates(&root)
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or("template not found")?;
    let rel = templates::create(&root, &tpl, &vars)?;
    // Index/navigation use workspace paths "<label>/<rel>".
    let wpath = format!("{}/{}", label, rel);
    engine.reindex_file(&wpath).map_err(err)?;
    Ok(wpath)
}

// ------------------------- extensions -------------------------

#[tauri::command]
fn ext_list(state: State<'_, AppState>) -> CmdResult<Vec<ExtensionInfo>> {
    Ok(extensions::list(&project_root(&state)?))
}

#[tauri::command]
fn ext_set_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> CmdResult<Vec<ExtensionInfo>> {
    extensions::set_enabled(&project_root(&state)?, &id, enabled)
}

#[tauri::command]
fn ext_install_example(state: State<'_, AppState>) -> CmdResult<Vec<ExtensionInfo>> {
    extensions::install_example(&project_root(&state)?)
}

#[tauri::command]
fn ext_snippets(state: State<'_, AppState>) -> CmdResult<Vec<Snippet>> {
    Ok(extensions::contributed_snippets(&project_root(&state)?))
}

/// Build the native application menu (macOS menu bar / Windows-Linux menu).
/// Custom items carry stable ids; selecting one emits `menu-action` to the UI,
/// which dispatches the corresponding command.
fn build_app_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let mi = |id: &str, label: &str, accel: Option<&str>| {
        let mut b = MenuItemBuilder::with_id(id.to_string(), label);
        if let Some(a) = accel {
            b = b.accelerator(a);
        }
        b.build(app)
    };

    let app_menu = SubmenuBuilder::new(app, "Photon")
        .item(&mi("about", "About Photon", None)?)
        .separator()
        .item(&mi("settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, Some("Hide Photon"))?)
        .item(&PredefinedMenuItem::quit(app, Some("Quit Photon"))?)
        .build()?;

    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&mi("open_folder", "Open Folder…", Some("CmdOrCtrl+O"))?)
        .item(&mi("new_template", "New from Template…", Some("CmdOrCtrl+N"))?)
        .item(&mi("save", "Save", Some("CmdOrCtrl+S"))?)
        .separator()
        .item(&PredefinedMenuItem::close_window(app, Some("Close Window"))?)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, Some("Undo"))?)
        .item(&PredefinedMenuItem::redo(app, Some("Redo"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some("Cut"))?)
        .item(&PredefinedMenuItem::copy(app, Some("Copy"))?)
        .item(&PredefinedMenuItem::paste(app, Some("Paste"))?)
        .item(&PredefinedMenuItem::select_all(app, Some("Select All"))?)
        .separator()
        .item(&mi("rename", "Rename Symbol…", Some("F2"))?)
        .build()?;

    let view_menu = SubmenuBuilder::new(app, "View")
        .item(&mi("search_everywhere", "Search Everywhere", Some("CmdOrCtrl+P"))?)
        .item(&mi("toggle_terminal", "Toggle Terminal", Some("CmdOrCtrl+`"))?)
        .item(&mi("view_explorer", "Explorer", None)?)
        .item(&mi("view_git", "Source Control", None)?)
        .item(&mi("view_database", "Database", None)?)
        .item(&mi("view_extensions", "Extensions", None)?)
        .build()?;

    let code_menu = SubmenuBuilder::new(app, "Code")
        .item(&mi("code_generate", "Generate…", Some("CmdOrCtrl+N"))?)
        .item(&mi("code_complete", "Code Completion", Some("CmdOrCtrl+Space"))?)
        .separator()
        .item(&mi("code_optimize_imports", "Optimize Imports", None)?)
        .item(&mi("code_reformat", "Reformat Code", Some("CmdOrCtrl+Alt+L"))?)
        .separator()
        .item(&mi("code_move_up", "Move Line Up", Some("Alt+Shift+Up"))?)
        .item(&mi("code_move_down", "Move Line Down", Some("Alt+Shift+Down"))?)
        .item(&mi("code_comment", "Comment with Line Comment", Some("CmdOrCtrl+/"))?)
        .build()?;

    let refactor_menu = SubmenuBuilder::new(app, "Refactor")
        .item(&mi("rename", "Rename…", Some("F2"))?)
        .item(&mi("refactor_extract_var", "Extract Variable…", Some("CmdOrCtrl+Alt+V"))?)
        .item(&mi("refactor_extract_method", "Extract Method…", Some("CmdOrCtrl+Alt+M"))?)
        .item(&mi("refactor_inline", "Inline…", Some("CmdOrCtrl+Alt+N"))?)
        .item(&mi("refactor_safe_delete", "Safe Delete…", None)?)
        .build()?;

    let laravel_menu = SubmenuBuilder::new(app, "Laravel")
        .item(&mi("laravel_generate", "Code Generation…", None)?)
        .item(&mi("laravel_new_model", "New Eloquent Model", None)?)
        .item(&mi("laravel_new_class", "New Class", None)?)
        .separator()
        .item(&mi("laravel_phpdoc", "Generate Model PHPDoc", None)?)
        .separator()
        .item(&mi("laravel_route_search", "Route Search", None)?)
        .item(&mi("laravel_artisan", "Run Artisan Command…", None)?)
        .separator()
        .item(&mi("laravel_missing_views", "Find Missing Translations", None)?)
        .build()?;

    let git_menu = SubmenuBuilder::new(app, "Git")
        .item(&mi("git_commit", "Commit…", Some("CmdOrCtrl+K"))?)
        .item(&mi("git_push", "Push…", Some("CmdOrCtrl+Shift+K"))?)
        .item(&mi("git_update", "Update Project…", Some("CmdOrCtrl+T"))?)
        .item(&mi("git_pull", "Pull…", None)?)
        .separator()
        .item(&mi("git_new_branch", "New Branch…", None)?)
        .item(&mi("git_branches", "Branches…", None)?)
        .item(&mi("git_stash", "Stash Changes…", None)?)
        .separator()
        .item(&mi("git_log", "Show Git Log", None)?)
        .build()?;

    let tools_menu = SubmenuBuilder::new(app, "Tools")
        .item(&mi("new_terminal", "New Terminal", None)?)
        .item(&mi("db_new_source", "New Data Source…", None)?)
        .build()?;

    let help_menu = SubmenuBuilder::new(app, "Help")
        .item(&mi("docs", "Photon Documentation", None)?)
        .build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_menu, &file_menu, &edit_menu, &view_menu, &code_menu, &refactor_menu,
            &laravel_menu, &git_menu, &tools_menu, &help_menu,
        ])
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Register sqlx Any drivers once (MySQL / Postgres / SQLite).
    sqlx::any::install_default_drivers();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .manage(DbManager::default())
        .manage(redis_client::RedisManager::default())
        .manage(debugger::DebugState::default())
        .manage(Terminals::default())
        .setup(|app| {
            let _ = app.get_webview_window("main");
            let handle = app.handle().clone();
            if let Ok(menu) = build_app_menu(&handle) {
                let _ = app.set_menu(menu);
            }
            // Forward native-menu selections to the UI.
            app.on_menu_event(move |app, event| {
                let _ = app.emit("menu-action", event.id().0.clone());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_project,
            reindex_path,
            close_project,
            list_projects,
            index_vendor,
            goto_laravel_key,
            goto_binding,
            list_files,
            read_file,
            save_file,
            file_symbols,
            search_everywhere,
            list_routes,
            goto_symbol,
            find_usages,
            plan_rename,
            plan_move_class,
            plan_change_signature,
            psr4_map,
            history_list,
            history_get,
            apply_rename,
            list_models,
            config_key,
            translation,
            missing_translations,
            list_bindings,
            list_events,
            list_jobs,
            list_artifacts,
            refactor_extract_variable,
            refactor_inline_variable,
            refactor_extract_method,
            refactor_safe_delete,
            member_completions,
            goto_member_def,
            goto_type,
            usages_popup,
            goto_implementations,
            generate_model_phpdoc,
            artisan_commands,
            run_artisan,
            run_test,
            lint_file,
            completion_data,
            schema_tables,
            blade_views,
            call_params,
            symbol_doc,
            return_type_fix,
            db_connect,
            redis_connect,
            redis_disconnect,
            redis_connections,
            redis_command,
            debug_listen,
            debug_command,
            debug_set_breakpoint,
            debug_remove_breakpoint,
            debug_property,
            path_to_workspace,
            db_disconnect,
            db_connections,
            db_schema,
            db_query,
            db_update_cell,
            git_is_repo,
            git_status,
            git_stage,
            git_unstage,
            git_commit,
            git_branches,
            git_checkout,
            git_create_branch,
            git_diff,
            git_log,
            git_push,
            git_pull,
            git_stash,
            git_stash_pop,
            git_graph,
            git_suggest_message,
            git_branches_detailed,
            git_update,
            git_blame,
            git_diff_sides,
            git_cherry_pick,
            git_compare,
            git_conflicts,
            git_resolve,
            git_discard,
            git_amend,
            git_reset,
            git_revert,
            git_file_diff,
            git_apply_hunk,
            git_conflict_versions,
            git_resolve_content,
            git_merge,
            git_branch_force,
            git_insights,
            git_line_status,
            git_pr_url,
            open_external,
            git_rebase_list,
            git_rebase_interactive,
            db_list_sources,
            db_save_source,
            db_delete_source,
            db_test_source,
            db_connect_source,
            term_spawn,
            term_write,
            term_resize,
            term_kill,
            settings_load,
            settings_save,
            system_stats,
            ai_chat,
            http_request,
            template_list,
            template_create,
            ext_list,
            ext_set_enabled,
            ext_install_example,
            ext_snippets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Photon IDE");
}
