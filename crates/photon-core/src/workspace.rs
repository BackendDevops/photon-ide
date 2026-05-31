//! Workspace: the (multi-root) project file model.
//!
//! Supports **several project roots open at once** (like VS Code workspaces).
//! Every file is addressed by a *workspace path* `"<label>/<relative>"`, where
//! `<label>` is the project folder name (de-duplicated). Because all roots feed
//! one shared index, navigation (go-to-def, usages, Cmd+click) works **across
//! projects** — clicking a class used in project X but defined in project Y
//! jumps straight there.

use crate::types::FileEntry;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

const HARD_SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "dist", "build", ".photon", ".idea", ".vscode",
];
const VENDOR_MARKERS: &[&str] = &["vendor/", "storage/framework/", "bootstrap/cache/"];

pub struct Root {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Default)]
pub struct Workspace {
    pub roots: Vec<Root>,
}

impl Workspace {
    pub fn new() -> Self {
        Workspace { roots: Vec::new() }
    }

    /// Back-compat single-root constructor.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let mut w = Workspace::new();
        w.add_root(root.into());
        w
    }

    /// Add a project root; returns its (unique) workspace label.
    pub fn add_root(&mut self, path: PathBuf) -> String {
        let base = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        let mut label = base.clone();
        let mut n = 2;
        while self.roots.iter().any(|r| r.label == label) {
            label = format!("{base} ({n})");
            n += 1;
        }
        self.roots.push(Root {
            label: label.clone(),
            path,
        });
        label
    }

    pub fn remove_root(&mut self, label: &str) {
        self.roots.retain(|r| r.label != label);
    }

    /// First root's absolute path (used as a default cwd, etc.).
    pub fn primary_path(&self) -> Option<&Path> {
        self.roots.first().map(|r| r.path.as_path())
    }

    pub fn is_laravel(&self) -> bool {
        self.roots.iter().any(|r| is_laravel_root(&r.path))
    }

    pub fn root_is_laravel(&self, label: &str) -> bool {
        self.roots
            .iter()
            .find(|r| r.label == label)
            .map(|r| is_laravel_root(&r.path))
            .unwrap_or(false)
    }

    /// All files across all roots, workspace-pathed.
    pub fn list_files(&self) -> Vec<FileEntry> {
        let mut out = Vec::new();
        for root in &self.roots {
            walk_root(&root.label, &root.path, &mut out);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Files for a single root (used when adding a project incrementally).
    pub fn list_files_for(&self, label: &str) -> Vec<FileEntry> {
        let mut out = Vec::new();
        if let Some(root) = self.roots.iter().find(|r| r.label == label) {
            walk_root(&root.label, &root.path, &mut out);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Walk each root's `vendor/` directory (bypassing .gitignore) for PHP files,
    /// skipping tests/docs to keep declaration indexing lean. Used for the
    /// deferred, declaration-level framework index.
    pub fn vendor_files(&self) -> Vec<FileEntry> {
        const SKIP: &[&str] = &["tests", "test", "Tests", "Test", "docs", "doc", "examples", ".github"];
        let mut out = Vec::new();
        for root in &self.roots {
            let vdir = root.path.join("vendor");
            if !vdir.is_dir() {
                continue;
            }
            let walker = WalkBuilder::new(&vdir)
                .git_ignore(false)
                .git_global(false)
                .hidden(true)
                .standard_filters(false)
                .filter_entry(|entry| {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if let Some(name) = entry.file_name().to_str() {
                            return !SKIP.contains(&name);
                        }
                    }
                    true
                })
                .build();
            for result in walker {
                let entry = match result {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("php") {
                    continue;
                }
                let rel = match path.strip_prefix(&root.path) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let md = entry.metadata().ok();
                out.push(FileEntry {
                    path: format!("{}/{}", root.label, normalize(rel)),
                    lang: "php".into(),
                    size: md.as_ref().map(|m| m.len()).unwrap_or(0),
                    is_vendor: true,
                    mtime: md.as_ref().map(mtime_secs).unwrap_or(0),
                });
            }
        }
        out
    }

    /// Resolve a workspace path `"<label>/<rel>"` to an absolute filesystem path.
    fn resolve(&self, wpath: &str) -> Option<PathBuf> {
        let (label, rel) = wpath.split_once('/')?;
        let root = self.roots.iter().find(|r| r.label == label)?;
        Some(root.path.join(rel))
    }

    /// Absolute filesystem path for a workspace path (`<label>/<rel>`).
    pub fn abs_path(&self, wpath: &str) -> Option<PathBuf> {
        self.resolve(wpath)
    }

    /// Map an absolute path back to a workspace path, if it lies under a root.
    pub fn wpath_of_abs(&self, abs: &str) -> Option<String> {
        let abs = Path::new(abs);
        for root in &self.roots {
            if let Ok(rel) = abs.strip_prefix(&root.path) {
                return Some(format!("{}/{}", root.label, normalize(rel)));
            }
        }
        None
    }

    /// Build a `FileEntry` for one workspace path by stat-ing it on disk.
    /// Returns `None` if it doesn't resolve or isn't a regular file.
    pub fn file_entry(&self, wpath: &str) -> Option<FileEntry> {
        let full = self.resolve(wpath)?;
        let md = std::fs::metadata(&full).ok()?;
        if !md.is_file() {
            return None;
        }
        Some(FileEntry {
            path: wpath.to_string(),
            lang: classify(&full),
            size: md.len(),
            is_vendor: VENDOR_MARKERS.iter().any(|m| wpath.contains(m)),
            mtime: mtime_secs(&md),
        })
    }

    pub fn read_file(&self, wpath: &str) -> std::io::Result<String> {
        match self.resolve(wpath) {
            Some(p) => std::fs::read_to_string(p),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unknown workspace root",
            )),
        }
    }

    pub fn write_file(&self, wpath: &str, contents: &str) -> std::io::Result<()> {
        let full = self.resolve(wpath).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "unknown workspace root")
        })?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, contents)
    }
}

pub fn is_laravel_root(path: &Path) -> bool {
    path.join("artisan").exists() && path.join("app").is_dir() && path.join("routes").is_dir()
}

fn walk_root(label: &str, root: &Path, out: &mut Vec<FileEntry>) {
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(false)
        .parents(false)
        .filter_entry(|entry| {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    return !HARD_SKIP_DIRS.contains(&name);
                }
            }
            true
        })
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let rel_str = normalize(rel);
        let lang = classify(path);
        if lang == "other" && !is_interesting_other(&rel_str) {
            continue;
        }
        let md = entry.metadata().ok();
        let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
        let mtime = md.as_ref().map(mtime_secs).unwrap_or(0);
        let is_vendor = VENDOR_MARKERS.iter().any(|m| rel_str.contains(m));
        out.push(FileEntry {
            path: format!("{label}/{rel_str}"),
            lang,
            size,
            is_vendor,
            mtime,
        });
    }
}

pub fn classify(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".blade.php") {
        return "blade".into();
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "php" => "php",
        "js" | "mjs" | "cjs" => "js",
        "ts" => "ts",
        "tsx" => "tsx",
        "jsx" => "jsx",
        "vue" => "vue",
        "json" => "json",
        "sql" => "sql",
        "md" => "markdown",
        "css" => "css",
        "html" => "html",
        "yml" | "yaml" => "yaml",
        "env" => "env",
        _ => "other",
    }
    .to_string()
}

fn is_interesting_other(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    matches!(base, "artisan" | ".env" | ".env.example" | "composer.json" | "Dockerfile")
        || base.starts_with(".env")
}

fn normalize(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Modification time as whole seconds since the UNIX epoch (0 if unavailable).
fn mtime_secs(md: &std::fs::Metadata) -> u64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
