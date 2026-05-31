//! Shared type vocabulary for Photon core.
//!
//! These mirror the conceptual model in `docs/02-module-design.md` and the
//! schema in `docs/03-database-schema.md`, kept deliberately small for the MVP.

use serde::{Deserialize, Serialize};

/// The kind of a code symbol. Maps to the `symbols.kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Namespace,
    Class,
    Interface,
    Trait,
    Enum,
    EnumCase,
    Function,
    Method,
    Property,
    Constant,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Namespace => "namespace",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Trait => "trait",
            SymbolKind::Enum => "enum",
            SymbolKind::EnumCase => "enum_case",
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Property => "property",
            SymbolKind::Constant => "constant",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "namespace" => SymbolKind::Namespace,
            "class" => SymbolKind::Class,
            "interface" => SymbolKind::Interface,
            "trait" => SymbolKind::Trait,
            "enum" => SymbolKind::Enum,
            "enum_case" => SymbolKind::EnumCase,
            "function" => SymbolKind::Function,
            "method" => SymbolKind::Method,
            "property" => SymbolKind::Property,
            "constant" => SymbolKind::Constant,
            _ => return None,
        })
    }
}

/// A symbol extracted from a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    /// Short name, e.g. `index`.
    pub name: String,
    /// Fully-qualified name where known, e.g. `App\Http\Controllers\UserController::index`.
    pub fqn: Option<String>,
    pub kind: SymbolKind,
    /// Workspace-relative path of the file the symbol lives in.
    pub file: String,
    /// Enclosing symbol name (class for a method/property), if any.
    pub container: Option<String>,
    /// 1-based line of the symbol's name.
    pub line: u32,
    /// 0-based byte offset of the name within the file (for precise navigation).
    pub name_offset: u32,
    /// Byte range of the whole declaration (for body-aware refactors). 0,0 if unknown.
    #[serde(default)]
    pub range_start: u32,
    #[serde(default)]
    pub range_end: u32,
}

/// A file in the workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// Workspace-relative path using forward slashes.
    pub path: String,
    pub lang: String,
    pub size: u64,
    pub is_vendor: bool,
    /// Last-modified time (seconds since UNIX epoch); 0 if unknown. Used for
    /// incremental warm-start reconciliation against the persistent index.
    #[serde(default)]
    pub mtime: u64,
}

/// A discovered Laravel route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub method: String,
    pub uri: String,
    pub name: Option<String>,
    /// Controller@method or "Closure".
    pub action: Option<String>,
    pub file: String,
    pub line: u32,
}

/// A unified Search Everywhere result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    /// "file" | "symbol" | "route".
    pub category: String,
    /// Primary label shown to the user.
    pub label: String,
    /// Secondary, dimmed detail (path, container, uri...).
    pub detail: String,
    pub file: String,
    pub line: u32,
    /// Fuzzy match score (higher is better) used for ranking.
    pub score: i64,
    /// Symbol kind string when category == "symbol".
    pub kind: Option<String>,
}

/// What a reference does at its use site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    /// `new Foo`, `extends Foo`, `implements Foo`, type hints, `Foo::class`.
    TypeRef,
    /// `Foo::bar()`, static call/access.
    StaticRef,
    /// `foo()` free function call.
    Call,
    /// `use App\Foo;` import.
    Import,
    /// `$x->method()` / `$x->prop` (dynamic member access by name).
    Member,
}

impl RefKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RefKind::TypeRef => "type_ref",
            RefKind::StaticRef => "static_ref",
            RefKind::Call => "call",
            RefKind::Import => "import",
            RefKind::Member => "member",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "type_ref" => RefKind::TypeRef,
            "static_ref" => RefKind::StaticRef,
            "call" => RefKind::Call,
            "import" => RefKind::Import,
            "member" => RefKind::Member,
            _ => RefKind::TypeRef,
        }
    }
}

/// A use-site of a name. Resolution links it to a definition by name/FQN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    /// The identifier text at the use site (short name).
    pub name: String,
    pub kind: RefKind,
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// Byte range of the identifier in the file (for precise edits).
    pub start: u32,
    pub end: u32,
}

/// One concrete text edit within a file (byte range → replacement).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub file: String,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub new_text: String,
    /// Preview of the line the edit lands on.
    pub preview: String,
    /// True when the engine is sure this refers to the renamed symbol.
    pub certain: bool,
}

/// A previewable, atomically-applied set of edits (rename, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeSet {
    pub title: String,
    pub edits: Vec<TextEdit>,
    pub files_affected: u32,
}

/// Summary returned after opening/indexing a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub root: String,
    pub files_indexed: u32,
    pub symbols: u32,
    pub routes: u32,
    pub references: u32,
    pub models: u32,
    pub is_laravel: bool,
    pub php_files: u32,
}

/// An open project root in the (multi-root) workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootInfo {
    pub label: String,
    pub path: String,
    pub is_laravel: bool,
}

/// A navigable location (file + 1-based line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: u32,
}

/// Lightweight model info for the Models panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub fqn: Option<String>,
    pub table: Option<String>,
    pub file: String,
    pub line: u32,
    pub fillable: Vec<String>,
    pub relations: Vec<RelationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationInfo {
    /// The relation method name, e.g. `posts`.
    pub method: String,
    /// hasMany / belongsTo / ...
    pub rel_type: String,
    /// Related model short name where resolvable.
    pub related: Option<String>,
    pub line: u32,
}

/// A config or translation key with its source location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEntry {
    pub key: String,
    /// For translations: the locale; empty for config.
    pub locale: String,
    pub file: String,
    pub line: u32,
}

/// One usage in the "Show Usages" popup, with a code-line preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageHit {
    pub file: String,
    pub line: u32,
    pub kind: String,
    /// Trimmed source line at the usage.
    pub preview: String,
    /// Enclosing symbol (method/class) name, if known.
    pub container: Option<String>,
}

/// Result for the floating usages popup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsagesResult {
    /// e.g. "Class \App\Services\Foo".
    pub title: String,
    pub total: u32,
    pub hits: Vec<UsageHit>,
}

/// An editor diagnostic (squiggle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
    pub message: String,
    /// error | warning | info
    pub severity: String,
}

/// Completion data the editor uses to offer Laravel-aware suggestions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionData {
    pub routes: Vec<String>,
    pub configs: Vec<String>,
    pub translations: Vec<String>,
    pub classes: Vec<String>,
    #[serde(default)]
    pub envs: Vec<String>,
    #[serde(default)]
    pub middlewares: Vec<String>,
    #[serde(default)]
    pub request_keys: Vec<String>,
}

/// A service-container binding (bind/singleton/instance/alias).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub abstract_name: String,
    pub concrete: Option<String>,
    /// bind | singleton | instance | alias
    pub kind: String,
    pub file: String,
    pub line: u32,
}

/// An event → listener wiring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventListener {
    pub event: String,
    pub listener: String,
    pub file: String,
    pub line: u32,
}

/// A queued job (ShouldQueue) and where it is dispatched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobInfo {
    pub name: String,
    pub fqn: Option<String>,
    pub queued: bool,
    pub file: String,
    pub line: u32,
}

/// A model factory or seeder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub name: String,
    /// factory | seeder
    pub kind: String,
    /// For factories: the related model short name where resolvable.
    pub related: Option<String>,
    pub file: String,
    pub line: u32,
}

/// A translation key missing in one or more locales.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingTranslation {
    pub key: String,
    pub present_in: Vec<String>,
    pub missing_in: Vec<String>,
}
