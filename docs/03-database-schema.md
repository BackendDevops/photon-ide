# 03 — Database Schema (SQLite Symbol & Intelligence Store)

The index is the brain. It must answer "where is X defined", "who uses Y", "what routes exist", "what columns does this model have" in **single-digit milliseconds**, survive crashes, and persist between sessions so warm starts skip rebuilding. SQLite (WAL mode, memory-mapped) is the system of record; this document is its schema.

## 1. Why SQLite

- **Embedded, zero-admin, single-file** per project — trivial to ship, back up, and delete.
- **Fast for our access pattern:** point lookups by interned id, range scans on indexed columns, FTS for fuzzy search.
- **Crash-safe** via WAL; combined with our own append-only delta log for recovery.
- **Queryable by Search Everywhere directly** — symbols, routes, models, and DB objects are all rows.

Open flags: `journal_mode=WAL`, `synchronous=NORMAL`, `mmap_size` tuned to project size, `cache_size` bounded by the memory budget, `foreign_keys=ON`.

## 2. Schema overview

```
files ──< file_symbols >── symbols ──< symbol_relations >── symbols
  │                           │
  │                           ├──< symbol_types        (inferred/declared types)
  │                           ├──< symbol_docs         (PHPDoc)
  │                           └──< symbol_attributes   (PHP 8 attributes)
references ─► symbols (def) + files (use site)
symbols_fts (FTS5 fuzzy)        names_trigram (prefix/substr)

laravel_routes  laravel_models  laravel_relations  laravel_bindings
laravel_views   laravel_config  laravel_translations  laravel_events  laravel_jobs

db_connections  db_objects (schemas/tables/columns/indexes)   // DB tools intel
project_meta  index_state  change_log  schema_migrations
```

## 3. Core tables

### Interning
All names (identifiers, FQNs, file paths) are interned to integer ids to keep the graph compact (a major part of the memory story).

```sql
CREATE TABLE strings (
  id     INTEGER PRIMARY KEY,
  text   TEXT NOT NULL UNIQUE
);
```

### files

```sql
CREATE TABLE files (
  id          INTEGER PRIMARY KEY,
  path_id     INTEGER NOT NULL REFERENCES strings(id),   -- workspace-relative
  lang        TEXT NOT NULL,           -- 'php','blade','js','ts','vue','sql',...
  size        INTEGER NOT NULL,
  mtime_ns    INTEGER NOT NULL,        -- for incremental staleness checks
  content_hash BLOB NOT NULL,          -- xxh3/blake3 of contents
  is_vendor   INTEGER NOT NULL DEFAULT 0,
  indexed_rev INTEGER NOT NULL,        -- index generation this file was built at
  parse_error INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_files_path ON files(path_id);
CREATE INDEX idx_files_lang ON files(lang);
CREATE INDEX idx_files_vendor ON files(is_vendor);
```

`mtime_ns` + `content_hash` are how warm start validates the index in O(changed files): walk file metadata, compare, re-index only deltas (see [04](./04-indexing-engine.md)).

### symbols

```sql
CREATE TABLE symbols (
  id           INTEGER PRIMARY KEY,
  name_id      INTEGER NOT NULL REFERENCES strings(id),   -- short name
  fqn_id       INTEGER REFERENCES strings(id),            -- fully-qualified
  kind         INTEGER NOT NULL,   -- enum: class,interface,trait,enum,method,
                                   -- function,property,const,enum_case,param,
                                   -- namespace,closure,anon_class,...
  file_id      INTEGER NOT NULL REFERENCES files(id),
  container_id INTEGER REFERENCES symbols(id),  -- enclosing class/namespace
  range_start  INTEGER NOT NULL,   -- byte offset
  range_end    INTEGER NOT NULL,
  name_start   INTEGER NOT NULL,   -- selection range (the identifier)
  name_end     INTEGER NOT NULL,
  visibility   INTEGER,            -- public/protected/private/0
  flags        INTEGER NOT NULL DEFAULT 0,  -- bitset: static,abstract,final,
                                            -- readonly,magic,deprecated,...
  signature_id INTEGER REFERENCES strings(id) -- rendered signature for hints
);
CREATE INDEX idx_symbols_name ON symbols(name_id, kind);
CREATE INDEX idx_symbols_fqn  ON symbols(fqn_id);
CREATE INDEX idx_symbols_file ON symbols(file_id);
CREATE INDEX idx_symbols_container ON symbols(container_id);
CREATE INDEX idx_symbols_kind ON symbols(kind);
```

### symbol_types — inferred & declared types

```sql
CREATE TABLE symbol_types (
  symbol_id   INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
  role        INTEGER NOT NULL,  -- 0=declared,1=inferred,2=phpdoc,3=return,4=param
  param_index INTEGER,           -- for parameters / return slot
  type_expr   TEXT NOT NULL,     -- normalized type string e.g. 'Collection<int,User>'
  nullable    INTEGER NOT NULL DEFAULT 0,
  confidence  INTEGER NOT NULL DEFAULT 100  -- 0..100, lower for dynamic
);
CREATE INDEX idx_symtypes_symbol ON symbol_types(symbol_id);
```

Generics, dynamic return types, and union/intersection types are stored as normalized `type_expr` strings the type engine ([05](./05-php-analysis-engine.md)) can re-parse cheaply.

### symbol_relations — the graph (inheritance, traits, implements)

```sql
CREATE TABLE symbol_relations (
  from_id  INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
  to_ref   INTEGER NOT NULL,    -- resolved symbol id OR unresolved name id
  resolved INTEGER NOT NULL,    -- 1 if to_ref is a symbol id, 0 if a name id
  rel      INTEGER NOT NULL     -- extends,implements,uses_trait,instantiates,
                                -- overrides,returns,param_of,...
);
CREATE INDEX idx_rel_from ON symbol_relations(from_id, rel);
CREATE INDEX idx_rel_to   ON symbol_relations(to_ref, rel, resolved);
```

`idx_rel_to` powers **Find Implementations / Find Subclasses** in one indexed scan.

### references — every usage (for Find Usages)

```sql
CREATE TABLE references (
  id          INTEGER PRIMARY KEY,
  symbol_id   INTEGER REFERENCES symbols(id) ON DELETE CASCADE, -- target (nullable if unresolved)
  unresolved_name_id INTEGER REFERENCES strings(id),            -- when not yet resolved
  file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  range_start INTEGER NOT NULL,
  range_end   INTEGER NOT NULL,
  kind        INTEGER NOT NULL,  -- read,write,call,instantiate,import,typehint,...
  context_id  INTEGER REFERENCES symbols(id)  -- enclosing symbol of the use site
);
CREATE INDEX idx_refs_symbol ON references(symbol_id);
CREATE INDEX idx_refs_file   ON references(file_id);
CREATE INDEX idx_refs_unresolved ON references(unresolved_name_id) WHERE symbol_id IS NULL;
```

Unresolved references are kept so that when a symbol later appears (or is renamed), we can resolve/repair without a full rescan — critical for refactoring correctness.

### PHPDoc & attributes

```sql
CREATE TABLE symbol_docs (
  symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
  summary   TEXT,
  raw       TEXT,         -- full docblock
  tags      TEXT          -- JSON: [{tag:'param',type:'int',name:'id'}, ...]
);

CREATE TABLE symbol_attributes (
  symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
  name_id   INTEGER NOT NULL REFERENCES strings(id),  -- attribute FQN
  args      TEXT  -- JSON of attribute arguments
);
CREATE INDEX idx_attr_symbol ON symbol_attributes(symbol_id);
CREATE INDEX idx_attr_name ON symbol_attributes(name_id);
```

## 4. Search indexes (Search Everywhere speed)

```sql
-- Fuzzy/full-text over symbol names + FQNs + file paths
CREATE VIRTUAL TABLE symbols_fts USING fts5(
  name, fqn, kind UNINDEXED, symbol_id UNINDEXED,
  tokenize = 'trigram'
);

-- File path search
CREATE VIRTUAL TABLE files_fts USING fts5(
  path, file_id UNINDEXED, tokenize = 'trigram'
);
```

Trigram tokenization gives substring + fuzzy matching that maps well to camelCase/`\Namespace\Class` queries. The navigation module merges FTS candidates with a CamelHumps fuzzy scorer in Rust and ranks by `score × recency × kind_weight`, streaming top-K (< 100 ms first paint).

## 5. Laravel intelligence tables

These are populated by the Laravel engine ([06](./06-laravel-intelligence.md)) and queried by navigation, completion, and Search Everywhere.

```sql
CREATE TABLE laravel_routes (
  id          INTEGER PRIMARY KEY,
  method      TEXT NOT NULL,         -- GET|POST|... (csv for multi)
  uri         TEXT NOT NULL,
  name        TEXT,                  -- named route
  action_symbol_id INTEGER REFERENCES symbols(id),  -- controller@method
  middleware  TEXT,                  -- JSON array
  file_id     INTEGER REFERENCES files(id),
  range_start INTEGER, range_end INTEGER,
  domain      TEXT
);
CREATE INDEX idx_routes_name ON laravel_routes(name);
CREATE INDEX idx_routes_uri  ON laravel_routes(uri);

CREATE TABLE laravel_models (
  symbol_id   INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
  table_name  TEXT,
  primary_key TEXT,
  connection  TEXT,
  fillable    TEXT,   -- JSON
  casts       TEXT,   -- JSON {col: type}
  db_object_id INTEGER REFERENCES db_objects(id)  -- link model ↔ real table
);

CREATE TABLE laravel_relations (
  id          INTEGER PRIMARY KEY,
  model_id    INTEGER NOT NULL REFERENCES laravel_models(symbol_id) ON DELETE CASCADE,
  method_name TEXT NOT NULL,         -- the relation method
  rel_type    TEXT NOT NULL,         -- hasMany,belongsTo,belongsToMany,morphTo,...
  related_model_id INTEGER REFERENCES laravel_models(symbol_id),
  pivot_table TEXT, foreign_key TEXT, local_key TEXT
);
CREATE INDEX idx_lrel_model ON laravel_relations(model_id);

CREATE TABLE laravel_bindings (
  id            INTEGER PRIMARY KEY,
  abstract      TEXT NOT NULL,        -- interface/abstract or string key
  concrete_symbol_id INTEGER REFERENCES symbols(id),
  binding_kind  TEXT,                 -- bind|singleton|instance|alias
  provider_symbol_id INTEGER REFERENCES symbols(id), -- service provider
  file_id INTEGER REFERENCES files(id), range_start INTEGER
);
CREATE INDEX idx_bind_abstract ON laravel_bindings(abstract);

CREATE TABLE laravel_views (    -- Blade templates & components
  id          INTEGER PRIMARY KEY,
  view_name   TEXT NOT NULL,         -- dotted name e.g. 'components.button'
  kind        TEXT NOT NULL,         -- view|component|layout
  file_id     INTEGER REFERENCES files(id),
  component_class_id INTEGER REFERENCES symbols(id), -- class-based component
  props       TEXT,                  -- JSON of @props / public props
  slots       TEXT                   -- JSON
);
CREATE INDEX idx_views_name ON laravel_views(view_name);

CREATE TABLE laravel_config (
  key_path    TEXT PRIMARY KEY,      -- 'services.stripe.key'
  file_id     INTEGER REFERENCES files(id),
  range_start INTEGER, value_kind TEXT
);

CREATE TABLE laravel_translations (
  key_path    TEXT NOT NULL,         -- 'auth.failed'
  locale      TEXT NOT NULL,
  file_id     INTEGER REFERENCES files(id),
  range_start INTEGER,
  PRIMARY KEY (key_path, locale)
);

CREATE TABLE laravel_events (
  id INTEGER PRIMARY KEY,
  event_symbol_id INTEGER REFERENCES symbols(id),
  listener_symbol_id INTEGER REFERENCES symbols(id),
  source TEXT  -- subscriber|provider|attribute|auto-discovery
);

CREATE TABLE laravel_jobs (
  symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
  queue TEXT, connection TEXT,
  dispatched_from TEXT  -- JSON of dispatch sites
);
```

The `db_object_id`/`db_objects` link is what lets Photon validate `$user->emale` against the real `users` table when a DB connection is attached — a PhpStorm-class feature.

## 6. Database-tools intelligence

```sql
CREATE TABLE db_connections (
  id INTEGER PRIMARY KEY, name TEXT, engine TEXT,  -- mysql|postgres|...
  host TEXT, port INTEGER, db_name TEXT,
  secret_ref TEXT  -- keychain handle, never the password
);

CREATE TABLE db_objects (
  id INTEGER PRIMARY KEY,
  conn_id INTEGER REFERENCES db_connections(id) ON DELETE CASCADE,
  parent_id INTEGER REFERENCES db_objects(id),  -- schema>table>column
  obj_type TEXT NOT NULL,   -- schema|table|view|column|index|fk|procedure
  name TEXT NOT NULL,
  data_type TEXT, nullable INTEGER, meta TEXT  -- JSON
);
CREATE INDEX idx_dbobj_conn ON db_objects(conn_id, obj_type);
CREATE INDEX idx_dbobj_parent ON db_objects(parent_id);
```

## 7. Bookkeeping, recovery, migrations

```sql
CREATE TABLE project_meta (
  key TEXT PRIMARY KEY, value TEXT
);  -- laravel_root, php_version, framework_version, last_opened, ...

CREATE TABLE index_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  generation INTEGER NOT NULL,     -- bumped per full reconcile
  schema_version INTEGER NOT NULL,
  last_full_scan_ns INTEGER,
  status TEXT                       -- building|ready|degraded
);

-- Append-only delta log for crash recovery (mirrors doc 04)
CREATE TABLE change_log (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  file_path TEXT NOT NULL,
  change TEXT NOT NULL,             -- created|modified|deleted|renamed
  mtime_ns INTEGER, content_hash BLOB,
  applied INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_changelog_unapplied ON change_log(applied) WHERE applied = 0;

CREATE TABLE schema_migrations (
  version INTEGER PRIMARY KEY, applied_at_ns INTEGER NOT NULL
);
```

## 8. Migration & versioning strategy

- `index_state.schema_version` gates compatibility. On open, if the stored schema version is older, run forward migrations (`schema_migrations`); if it's *newer* (user downgraded), discard and rebuild.
- **Extractor versioning:** each extractor (php, laravel, each language) has a version; bumping it marks affected files stale so only those re-index — we don't nuke the whole index when, say, the Blade extractor improves.
- Index files live in a per-project cache dir keyed by absolute path hash, so multiple projects don't collide and deleting one is trivial.

## 9. Access patterns → guaranteed-fast queries

| Feature | Query shape | Index used |
|---|---|---|
| Go to definition | `symbols WHERE fqn_id = ?` | `idx_symbols_fqn` |
| Find usages | `references WHERE symbol_id = ?` | `idx_refs_symbol` |
| Find implementations | `symbol_relations WHERE to_ref=? AND rel=implements` | `idx_rel_to` |
| Search Everywhere (symbol) | `symbols_fts MATCH ?` + Rust rerank | FTS5 trigram |
| Route navigation | `laravel_routes WHERE name=? OR uri LIKE ?` | `idx_routes_name/uri` |
| Model columns | `laravel_models` + `db_objects` join | `idx_dbobj_parent` |
| Stale check on warm start | `files` scan vs FS mtime | `idx_files_path` |

Every headline navigation feature resolves to one indexed lookup. That is the schema's entire purpose.

→ Next: [04 — Indexing Engine](./04-indexing-engine.md)
