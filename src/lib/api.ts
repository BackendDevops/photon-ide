// Typed bindings over the Tauri command layer (src-tauri/src/lib.rs).
// Field names are snake_case to match the Rust serde output.

import { invoke } from "@tauri-apps/api/core";

export type SymbolKind =
  | "namespace"
  | "class"
  | "interface"
  | "trait"
  | "enum"
  | "enum_case"
  | "function"
  | "method"
  | "property"
  | "constant";

export interface Symbol {
  name: string;
  fqn: string | null;
  kind: SymbolKind;
  file: string;
  container: string | null;
  line: number;
  name_offset: number;
}

export interface FileEntry {
  path: string;
  lang: string;
  size: number;
  is_vendor: boolean;
}

export interface HttpResponse {
  status: number;
  status_text: string;
  headers: [string, string][];
  body: string;
  duration_ms: number;
  size: number;
}

export interface Route {
  method: string;
  uri: string;
  name: string | null;
  action: string | null;
  file: string;
  line: number;
}

export interface SearchHit {
  category: "file" | "symbol" | "route";
  label: string;
  detail: string;
  file: string;
  line: number;
  score: number;
  kind: string | null;
}

export interface RootInfo {
  label: string;
  path: string;
  is_laravel: boolean;
}
export interface Location {
  file: string;
  line: number;
}

export interface ProjectSummary {
  root: string;
  files_indexed: number;
  symbols: number;
  routes: number;
  references: number;
  models: number;
  is_laravel: boolean;
  php_files: number;
}

export interface Reference {
  name: string;
  kind: "type_ref" | "static_ref" | "call" | "import" | "member";
  file: string;
  line: number;
  column: number;
  start: number;
  end: number;
}

export interface TextEdit {
  file: string;
  start: number;
  end: number;
  line: number;
  new_text: string;
  preview: string;
  certain: boolean;
}

export interface ChangeSet {
  title: string;
  edits: TextEdit[];
  files_affected: number;
}

export interface RelationInfo {
  method: string;
  rel_type: string;
  related: string | null;
  line: number;
}

export interface ModelInfo {
  name: string;
  fqn: string | null;
  table: string | null;
  file: string;
  line: number;
  fillable: string[];
  relations: RelationInfo[];
}

export interface KeyEntry {
  key: string;
  locale: string;
  file: string;
  line: number;
}

export interface MissingTranslation {
  key: string;
  present_in: string[];
  missing_in: string[];
}

// ---- Database tools ----
export interface DbColumn {
  name: string;
  data_type: string;
  nullable: boolean;
}
export interface DbTable {
  name: string;
  columns: DbColumn[];
}
export interface DbSchema {
  engine: string;
  tables: DbTable[];
}
export interface QueryResult {
  columns: string[];
  rows: string[][];
  row_count: number;
  affected: number | null;
}

// ---- Git ----
export interface GitFileStatus {
  path: string;
  status: string;
  staged: boolean;
  label: string;
}
export interface GitStatus {
  branch: string;
  ahead: number;
  behind: number;
  files: GitFileStatus[];
  clean: boolean;
}
export interface GitCommit {
  hash: string;
  short: string;
  author: string;
  date: string;
  subject: string;
}
export interface Branch {
  name: string;
  current: boolean;
}
export interface GraphCommit {
  hash: string;
  short: string;
  parents: string[];
  author: string;
  email: string;
  date: string;
  subject: string;
  refs: string[];
  lane: number;
  color: number;
}
export interface BranchInfo {
  name: string;
  current: boolean;
  remote: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  last_commit: string;
}

export interface Binding {
  abstract_name: string;
  concrete: string | null;
  kind: string;
  file: string;
  line: number;
}
export interface EventListener {
  event: string;
  listener: string;
  file: string;
  line: number;
}
export interface JobInfo {
  name: string;
  fqn: string | null;
  queued: boolean;
  file: string;
  line: number;
}
export interface Diagnostic {
  line: number;
  col: number;
  end_col: number;
  message: string;
  severity: string;
}
export interface CompletionData {
  routes: string[];
  configs: string[];
  translations: string[];
  classes: string[];
  envs: string[];
  middlewares: string[];
  request_keys: string[];
}
export interface BlameLine {
  line: number;
  short: string;
  author: string;
  summary: string;
}
export interface DiffSides {
  original: string;
  modified: string;
}
export interface ConflictVersions {
  base: string;
  ours: string;
  theirs: string;
  working: string;
  ours_label: string;
  theirs_label: string;
}
export interface Insights {
  total_commits: number;
  contributors: [string, number][];
  activity: [string, number][];
  files: [string, number][];
}
export interface SymbolDoc {
  name: string;
  kind: string;
  signature: string;
  params: [string, string][];
  return_type: string;
  doc: string;
  source: string;
}
export interface ReturnFix {
  ty: string;
  line: number;
  col: number;
}
export interface DebugStackFrame {
  file: string;
  line: number;
  func: string;
}
export interface DebugVariable {
  name: string;
  ty: string;
  value: string;
}
export interface DebugBreak {
  file: string;
  line: number;
  stack: DebugStackFrame[];
  vars: DebugVariable[];
}
export interface UsageHit {
  file: string;
  line: number;
  kind: string;
  preview: string;
  container: string | null;
}
export interface UsagesResult {
  title: string;
  total: number;
  hits: UsageHit[];
}

// ---- Templates & extensions ----
export interface TemplateField {
  key: string;
  label: string;
  default: string;
}
export interface Template {
  id: string;
  label: string;
  category: string;
  filename: string;
  body: string;
  fields: TemplateField[];
  source: string;
}
export interface ExtensionInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  enabled: boolean;
  template_count: number;
  snippet_count: number;
}

// ---- Data sources (advanced DB) ----
export interface DataSource {
  id: string;
  name: string;
  driver: "mysql" | "mariadb" | "postgres" | "sqlite";
  host: string;
  port: number;
  user: string;
  database: string;
  sqlite_path: string;
  save_password: boolean;
  password: string | null;
}

export const api = {
  openProject: (path: string) =>
    invoke<ProjectSummary>("open_project", { path }),
  closeProject: (label: string) =>
    invoke<ProjectSummary>("close_project", { label }),
  listProjects: () => invoke<RootInfo[]>("list_projects"),
  indexVendor: () => invoke<number>("index_vendor"),
  reindexPath: (path: string) => invoke<void>("reindex_path", { path }),
  gotoLaravelKey: (kind: string, key: string) =>
    invoke<Location | null>("goto_laravel_key", { kind, key }),
  gotoBinding: (name: string) =>
    invoke<Location | null>("goto_binding", { name }),
  listFiles: () => invoke<FileEntry[]>("list_files"),
  readFile: (path: string) => invoke<string>("read_file", { path }),
  saveFile: (path: string, contents: string) =>
    invoke<void>("save_file", { path, contents }),
  fileSymbols: (path: string) => invoke<Symbol[]>("file_symbols", { path }),
  searchEverywhere: (query: string) =>
    invoke<SearchHit[]>("search_everywhere", { query }),
  listRoutes: () => invoke<Route[]>("list_routes"),
  gotoSymbol: (name: string) => invoke<Symbol[]>("goto_symbol", { name }),

  // navigation + refactoring
  findUsages: (name: string) => invoke<Reference[]>("find_usages", { name }),
  planRename: (oldName: string, newName: string) =>
    invoke<ChangeSet>("plan_rename", { old: oldName, newName }),
  planMoveClass: (className: string, newNs: string) =>
    invoke<ChangeSet>("plan_move_class", { class: className, newNs }),
  planChangeSignature: (file: string, line: number, newParams: string) =>
    invoke<ChangeSet>("plan_change_signature", { file, line, newParams }),
  psr4Map: () => invoke<[string, string][]>("psr4_map"),
  historyList: (path: string) => invoke<number[]>("history_list", { path }),
  historyGet: (path: string, ts: number) => invoke<string>("history_get", { path, ts }),
  applyRename: (changeset: ChangeSet, accepted?: number[]) =>
    invoke<number>("apply_rename", { changeset, accepted: accepted ?? null }),

  // Laravel depth
  listModels: () => invoke<ModelInfo[]>("list_models"),
  configKey: (key: string) => invoke<KeyEntry | null>("config_key", { key }),
  translation: (key: string) => invoke<KeyEntry[]>("translation", { key }),
  missingTranslations: () =>
    invoke<MissingTranslation[]>("missing_translations"),
  listBindings: () => invoke<Binding[]>("list_bindings"),
  listEvents: () => invoke<EventListener[]>("list_events"),
  listJobs: () => invoke<JobInfo[]>("list_jobs"),

  // refactorings
  applyChangeset: (changeset: ChangeSet, accepted?: number[]) =>
    invoke<number>("apply_rename", { changeset, accepted: accepted ?? null }),
  refactorExtractVariable: (file: string, selStart: number, selEnd: number, newName: string, line: number) =>
    invoke<ChangeSet>("refactor_extract_variable", { file, selStart, selEnd, newName, line }),
  refactorInlineVariable: (file: string, varName: string) =>
    invoke<ChangeSet>("refactor_inline_variable", { file, var: varName }),
  refactorSafeDelete: (name: string) =>
    invoke<ChangeSet>("refactor_safe_delete", { name }),
  refactorExtractMethod: (file: string, selStart: number, selEnd: number, methodName: string, line: number) =>
    invoke<ChangeSet>("refactor_extract_method", { file, selStart, selEnd, methodName, line }),

  // type intelligence
  memberCompletions: (file: string, offset: number, receiver: string) =>
    invoke<Symbol[]>("member_completions", { file, offset, receiver }),
  usagesPopup: (name: string) => invoke<UsagesResult>("usages_popup", { name }),
  gotoImplementations: (name: string) =>
    invoke<UsagesResult>("goto_implementations", { name }),
  gotoMemberDef: (file: string, offset: number, chain: string, member: string) =>
    invoke<Location | null>("goto_member_def", { file, offset, chain, member }),
  gotoType: (file: string, offset: number, chain: string) =>
    invoke<Location | null>("goto_type", { file, offset, chain }),
  generateModelPhpdoc: (file: string) =>
    invoke<ChangeSet>("generate_model_phpdoc", { file }),
  artisanCommands: () => invoke<string[]>("artisan_commands"),
  runArtisan: (args: string) => invoke<string>("run_artisan", { args }),
  runTest: (path: string, filter?: string) =>
    invoke<{ passed: boolean; output: string }>("run_test", { path, filter: filter ?? null }),

  // diagnostics + completion
  lintFile: (path: string) => invoke<Diagnostic[]>("lint_file", { path }),
  completionData: () => invoke<CompletionData>("completion_data"),
  schemaTables: () => invoke<[string, string[]][]>("schema_tables"),
  bladeViews: () => invoke<string[]>("blade_views"),
  callParams: (name: string) => invoke<string[]>("call_params", { name }),
  symbolDoc: (name: string) => invoke<SymbolDoc | null>("symbol_doc", { name }),
  returnTypeFix: (path: string, line: number) =>
    invoke<ReturnFix | null>("return_type_fix", { path, line }),

  // database tools
  dbConnect: (name: string, url: string) =>
    invoke<string>("db_connect", { name, url }),
  dbDisconnect: (name: string) => invoke<void>("db_disconnect", { name }),
  dbConnections: () => invoke<string[]>("db_connections"),
  dbSchema: (name: string) => invoke<DbSchema>("db_schema", { name }),
  dbQuery: (name: string, sql: string) =>
    invoke<QueryResult>("db_query", { name, sql }),
  dbUpdateCell: (
    name: string,
    table: string,
    column: string,
    value: string,
    pkColumn: string,
    pkValue: string
  ) =>
    invoke<number>("db_update_cell", { name, table, column, value, pkColumn, pkValue }),

  // xdebug (DBGp debugger)
  debugListen: () => invoke<void>("debug_listen"),
  debugCommand: (verb: "run" | "step_into" | "step_over" | "step_out" | "stop") =>
    invoke<void>("debug_command", { verb }),
  debugSetBreakpoint: (path: string, line: number, condition?: string) =>
    invoke<void>("debug_set_breakpoint", { path, line, condition: condition ?? null }),
  debugRemoveBreakpoint: (path: string, line: number) =>
    invoke<void>("debug_remove_breakpoint", { path, line }),
  debugProperty: (name: string) => invoke<void>("debug_property", { name }),
  pathToWorkspace: (abs: string) =>
    invoke<string | null>("path_to_workspace", { abs }),

  // redis (NoSQL console)
  redisConnect: (name: string, url: string) =>
    invoke<string>("redis_connect", { name, url }),
  redisDisconnect: (name: string) => invoke<void>("redis_disconnect", { name }),
  redisCommand: (name: string, parts: string[]) =>
    invoke<string>("redis_command", { name, parts }),

  // git
  gitIsRepo: () => invoke<boolean>("git_is_repo"),
  gitStatus: () => invoke<GitStatus>("git_status"),
  gitStage: (paths: string[]) => invoke<void>("git_stage", { paths }),
  gitUnstage: (paths: string[]) => invoke<void>("git_unstage", { paths }),
  gitCommit: (message: string) => invoke<string>("git_commit", { message }),
  gitBranches: () => invoke<Branch[]>("git_branches"),
  gitCheckout: (branch: string) => invoke<string>("git_checkout", { branch }),
  gitCreateBranch: (name: string) =>
    invoke<string>("git_create_branch", { name }),
  gitDiff: (file: string) => invoke<string>("git_diff", { file }),
  gitLog: (limit: number) => invoke<GitCommit[]>("git_log", { limit }),
  gitPush: () => invoke<string>("git_push"),
  gitPull: () => invoke<string>("git_pull"),
  gitStash: () => invoke<string>("git_stash"),
  gitStashPop: () => invoke<string>("git_stash_pop"),
  gitGraph: (limit: number) => invoke<GraphCommit[]>("git_graph", { limit }),
  gitSuggestMessage: () => invoke<string>("git_suggest_message"),
  gitBlame: (file: string) => invoke<BlameLine[]>("git_blame", { file }),
  gitDiffSides: (file: string) => invoke<DiffSides>("git_diff_sides", { file }),
  gitCherryPick: (hash: string) => invoke<string>("git_cherry_pick", { hash }),
  gitCompare: (base: string, head: string) =>
    invoke<string>("git_compare", { base, head }),
  gitConflicts: () => invoke<string[]>("git_conflicts"),
  gitResolve: (file: string, side: "ours" | "theirs") =>
    invoke<string>("git_resolve", { file, side }),
  gitDiscard: (file: string) => invoke<string>("git_discard", { file }),
  gitAmend: (message: string) => invoke<string>("git_amend", { message }),
  gitReset: (target: string, mode: "soft" | "mixed" | "hard") =>
    invoke<string>("git_reset", { target, mode }),
  gitRevert: (hash: string) => invoke<string>("git_revert", { hash }),
  gitFileDiff: (file: string, staged: boolean) =>
    invoke<string>("git_file_diff", { file, staged }),
  gitApplyHunk: (patch: string, reverse: boolean) =>
    invoke<string>("git_apply_hunk", { patch, reverse }),
  gitConflictVersions: (file: string) =>
    invoke<ConflictVersions>("git_conflict_versions", { file }),
  gitResolveContent: (file: string, content: string) =>
    invoke<string>("git_resolve_content", { file, content }),
  gitMerge: (branch: string) => invoke<string>("git_merge", { branch }),
  gitBranchForce: (name: string, target: string) =>
    invoke<string>("git_branch_force", { name, target }),
  gitInsights: () => invoke<Insights>("git_insights"),
  gitLineStatus: (file: string) =>
    invoke<{ added: number[]; modified: number[]; deleted: number[] }>("git_line_status", { file }),
  gitRebaseList: (base: string) => invoke<GitCommit[]>("git_rebase_list", { base }),
  gitRebaseInteractive: (base: string, todo: string) =>
    invoke<string>("git_rebase_interactive", { base, todo }),
  gitPrUrl: () => invoke<string>("git_pr_url"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),

  // AI
  aiChat: (
    baseUrl: string,
    apiKey: string,
    model: string,
    messages: { role: string; content: string }[],
    context?: string
  ) =>
    invoke<string>("ai_chat", {
      baseUrl,
      apiKey,
      model,
      messages,
      context: context ?? null,
    }),

  // HTTP API client (bottom dock)
  httpRequest: (
    method: string,
    url: string,
    headers: [string, string][],
    body?: string
  ) =>
    invoke<HttpResponse>("http_request", {
      method,
      url,
      headers,
      body: body ?? null,
    }),

  // system
  systemStats: () =>
    invoke<{ php_version: string; memory_mb: number; indexed_files: number }>(
      "system_stats"
    ),
  gitBranchesDetailed: () => invoke<BranchInfo[]>("git_branches_detailed"),
  gitUpdate: () => invoke<string>("git_update"),

  // data sources
  dbListSources: () => invoke<DataSource[]>("db_list_sources"),
  dbSaveSource: (source: DataSource) =>
    invoke<DataSource[]>("db_save_source", { source }),
  dbDeleteSource: (id: string) =>
    invoke<DataSource[]>("db_delete_source", { id }),
  dbTestSource: (source: DataSource, password?: string) =>
    invoke<string>("db_test_source", { source, password: password ?? null }),
  dbConnectSource: (id: string, password?: string) =>
    invoke<string>("db_connect_source", { id, password: password ?? null }),

  // terminal
  termSpawn: (cwd: string | null, cols: number, rows: number) =>
    invoke<string>("term_spawn", { cwd, cols, rows }),
  termWrite: (id: string, data: string) =>
    invoke<void>("term_write", { id, data }),
  termResize: (id: string, cols: number, rows: number) =>
    invoke<void>("term_resize", { id, cols, rows }),
  termKill: (id: string) => invoke<void>("term_kill", { id }),

  // templates
  templateList: () => invoke<Template[]>("template_list"),
  templateCreate: (templateId: string, vars: Record<string, string>) =>
    invoke<string>("template_create", { templateId, vars }),

  // extensions
  extList: () => invoke<ExtensionInfo[]>("ext_list"),
  extSetEnabled: (id: string, enabled: boolean) =>
    invoke<ExtensionInfo[]>("ext_set_enabled", { id, enabled }),
  extInstallExample: () => invoke<ExtensionInfo[]>("ext_install_example"),
};

// Map a Photon language id to a Monaco language id.
export function monacoLang(lang: string): string {
  switch (lang) {
    case "php":
    case "blade":
      return "php";
    case "ts":
    case "tsx":
      return "typescript";
    case "js":
    case "jsx":
      return "javascript";
    case "json":
      return "json";
    case "sql":
      return "sql";
    case "css":
      return "css";
    case "html":
      return "html";
    case "markdown":
      return "markdown";
    case "yaml":
      return "yaml";
    case "vue":
      return "html";
    default:
      return "plaintext";
  }
}
