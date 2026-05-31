//! Extensions (a safe, declarative slice of the Plugin SDK in docs/07).
//!
//! Extensions live in `.photon/extensions/<id>/extension.json` and *declare*
//! contributions — templates and snippets — rather than executing arbitrary
//! code. This is the v1 extension surface: useful and fully sandboxed by
//! construction. The full out-of-process plugin runtime is docs/07.

use crate::templates::Template;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Snippet {
    pub prefix: String,
    #[serde(default)]
    pub language: String,
    pub body: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Contributes {
    #[serde(default)]
    pub templates: Vec<Template>,
    #[serde(default)]
    pub snippets: Vec<Snippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub contributes: Contributes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub template_count: usize,
    pub snippet_count: usize,
}

fn ext_dir(root: &str) -> PathBuf {
    PathBuf::from(root).join(".photon").join("extensions")
}

fn state_path(root: &str) -> PathBuf {
    PathBuf::from(root).join(".photon").join("extensions-state.json")
}

fn load_state(root: &str) -> HashMap<String, bool> {
    std::fs::read_to_string(state_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_state(root: &str, state: &HashMap<String, bool>) -> Result<(), String> {
    let path = state_path(root);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Read every extension manifest under `.photon/extensions`.
pub fn load_manifests(root: &str) -> Vec<ExtensionManifest> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(ext_dir(root)) {
        for e in entries.flatten() {
            let manifest = e.path().join("extension.json");
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                if let Ok(m) = serde_json::from_str::<ExtensionManifest>(&text) {
                    out.push(m);
                }
            }
        }
    }
    out
}

pub fn list(root: &str) -> Vec<ExtensionInfo> {
    let state = load_state(root);
    load_manifests(root)
        .into_iter()
        .map(|m| ExtensionInfo {
            enabled: *state.get(&m.id).unwrap_or(&true),
            template_count: m.contributes.templates.len(),
            snippet_count: m.contributes.snippets.len(),
            id: m.id,
            name: m.name,
            version: m.version,
            description: m.description,
            author: m.author,
        })
        .collect()
}

pub fn set_enabled(root: &str, id: &str, enabled: bool) -> Result<Vec<ExtensionInfo>, String> {
    let mut state = load_state(root);
    state.insert(id.to_string(), enabled);
    save_state(root, &state)?;
    Ok(list(root))
}

/// Templates contributed by *enabled* extensions (source tagged with ext id).
pub fn contributed_templates(root: &str) -> Vec<Template> {
    let state = load_state(root);
    let mut out = Vec::new();
    for m in load_manifests(root) {
        if !*state.get(&m.id).unwrap_or(&true) {
            continue;
        }
        for mut t in m.contributes.templates {
            t.source = format!("ext:{}", m.id);
            out.push(t);
        }
    }
    out
}

/// Snippets from enabled extensions.
pub fn contributed_snippets(root: &str) -> Vec<Snippet> {
    let state = load_state(root);
    let mut out = Vec::new();
    for m in load_manifests(root) {
        if !*state.get(&m.id).unwrap_or(&true) {
            continue;
        }
        out.extend(m.contributes.snippets);
    }
    out
}

/// Install a bundled example extension so the panel demonstrates the system.
pub fn install_example(root: &str) -> Result<Vec<ExtensionInfo>, String> {
    let dir = ext_dir(root).join("laravel-extras");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("extension.json"), EXAMPLE_MANIFEST).map_err(|e| e.to_string())?;
    Ok(list(root))
}

const EXAMPLE_MANIFEST: &str = r#"{
  "id": "laravel-extras",
  "name": "Laravel Extras",
  "version": "1.0.0",
  "author": "Photon",
  "description": "Adds Service & Action class templates and handy Laravel snippets.",
  "contributes": {
    "templates": [
      {
        "id": "service-class",
        "label": "Service Class",
        "category": "Laravel Extras",
        "filename": "app/Services/{{name}}.php",
        "fields": [{ "key": "name", "label": "Name", "default": "" }],
        "body": "<?php\n\nnamespace App\\Services;\n\nclass {{name}}\n{\n    public function __construct()\n    {\n        //\n    }\n}\n"
      },
      {
        "id": "action-class",
        "label": "Single Action",
        "category": "Laravel Extras",
        "filename": "app/Actions/{{name}}.php",
        "fields": [{ "key": "name", "label": "Name", "default": "" }],
        "body": "<?php\n\nnamespace App\\Actions;\n\nclass {{name}}\n{\n    public function handle(): void\n    {\n        //\n    }\n}\n"
      }
    ],
    "snippets": [
      { "prefix": "dd", "language": "php", "body": "dd($1);", "description": "dump and die" },
      { "prefix": "route", "language": "php", "body": "Route::get('$1', [$2::class, '$3']);", "description": "GET route" }
    ]
  }
}
"#;
