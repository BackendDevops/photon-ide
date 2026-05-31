# 04 — Indexing Engine Design

The indexer turns a directory of files into the queryable intelligence store in [03](./03-database-schema.md). It must scale to **1M+ files**, update **incrementally** in milliseconds on edit, **persist** between sessions so warm starts skip work, and **never** block the UI or starve the foreground. This is the engine that makes everything else feel instant.

## 1. Design goals & the numbers they imply

| Goal | Target | Consequence for design |
|---|---|---|
| First-open responsiveness | Editable < 1.2 s even on 1M files | Don't block on full scan; index lazily + in priority order |
| Warm start | < 600 ms to live intelligence | Persist index; validate by mtime/hash, re-index only deltas |
| Edit latency | Symbols current < 50 ms after pause | Incremental per-file re-index off the edit pipeline |
| Memory | Bounded regardless of project size | Stream files; never hold all ASTs; store to SQLite |
| Background politeness | Never lag the editor | Low-priority pool, cancellation, backpressure |

## 2. The indexing pipeline

```
                 ┌── priority queue (open files > visible > project > vendor) ──┐
file events ─►   │                                                              │
mtime scan  ─►   │   dedup + debounce  ─►  read + hash  ─►  changed?            │
                 └───────────────────────────────┬──────────────────────────────┘
                                                  │ yes
                              ┌───────────────────▼───────────────────┐
                              │ extract (per-language, on CPU pool)    │
                              │  tree-sitter parse → extractor →       │
                              │  {symbols, refs, types, relations,     │
                              │   laravel facts}                        │
                              └───────────────────┬───────────────────┘
                                                  │ FileDelta
                              ┌───────────────────▼───────────────────┐
                              │ apply transactionally to SQLite        │
                              │  (delete old rows for file → insert)   │
                              │  + append to change_log                │
                              └───────────────────┬───────────────────┘
                                                  │
                              resolve unresolved refs touching this file
                                                  │
                              emit IndexProgress / invalidate caches
```

Each stage runs on the appropriate pool (IO for read/hash/SQLite, CPU/rayon for parse/extract), all cancellable, all backpressured.

## 3. Incrementality — the core idea

We index **per file** and store the unit of work as a `FileDelta`. Re-indexing a file is: *delete all rows owned by that file, insert the freshly extracted rows.* Because every table carries `file_id` with `ON DELETE CASCADE`, a single delete cleans up symbols, refs, types, relations, and Laravel facts for that file atomically. No global recompute.

```rust
struct FileDelta {
    file: FileId,
    mtime_ns: i64,
    content_hash: Hash,
    symbols: Vec<SymbolRow>,
    references: Vec<RefRow>,          // some unresolved
    types: Vec<TypeRow>,
    relations: Vec<RelRow>,           // some unresolved (to a name, not an id)
    laravel: Option<LaravelFacts>,
}
```

### Two-phase resolution
Extraction is **local** (single file, no cross-file lookups) so it's embarrassingly parallel and fast. Cross-file linking (resolving `extends Foo` to a symbol id, a route's `action` to a controller method) happens in a cheap **resolve phase** that only touches symbols whose names match the newly added/removed symbols — driven by the `references(unresolved_name_id)` and `symbol_relations(resolved=0)` indexes. When `class Foo` appears, we resolve exactly the references waiting on the name "Foo", not the world.

This separation is why a 1M-file project can index in parallel without lock contention, and why renaming a class doesn't trigger a global rescan.

## 4. Scale to 1M+ files

1. **Lazy, prioritized indexing.** On first open we don't scan everything before letting you work. The priority queue indexes: open files → files in visible folders → the rest of project source → `vendor`/generated code last. You get intelligence on *your* code in seconds; the long tail fills in behind a progress indicator.
2. **Sharded parallel extraction.** The project file list is partitioned across the rayon pool; SQLite writes are funneled through a single writer task with batched transactions (thousands of rows per commit) to avoid WAL contention.
3. **Vendor handling.** `vendor/` is indexed at lower priority and lower fidelity by default (declarations yes, full reference graph optional), with a per-project toggle. Most navigation needs vendor *definitions*, not vendor *usages*.
4. **Bounded memory.** Files stream through the pipeline; we hold only the ASTs currently being extracted (a few per core), then drop them. Nothing accumulates. The output goes to disk (SQLite), not RAM.
5. **Generated/excluded paths.** `.gitignore`, `node_modules`, build dirs, and user-configured excludes are skipped or deprioritized.

A realistic Laravel monorepo (app + vendor + frontend) of ~150k files cold-indexes its *source* in seconds and finishes vendor in the background; the editor is usable the whole time.

## 5. Persistence & warm start

The index is written to a per-project SQLite file (see [03](./03-database-schema.md) §8). On reopen:

1. Load `index_state`; if schema/extractor versions are compatible, treat the index as authoritative.
2. **Validate by metadata, not content:** walk the file tree collecting `(path, mtime_ns, size)`. Compare against `files`. Only files whose mtime/size differ get hashed; only those whose hash differs get re-extracted. Deleted files cascade-delete; new files enqueue.
3. The editor is live immediately on the *old* index; the delta reconcile runs in the background and patches it. In practice a warm start touches a handful of files → live intelligence in well under a second.

This "trust then verify" approach is the difference between a 600 ms warm start and a multi-second rescan.

## 6. File watching & change coalescing

- Native watchers (FSEvents/RDCW/inotify) via the workspace `Watcher` trait.
- Events are **debounced** (~50–150 ms) and **coalesced**: a burst of saves, a `git checkout`, or `composer install` is collapsed into a batch and processed as one reconcile pass rather than thousands of individual deltas.
- **Rename detection:** create+delete pairs with matching content hash are recognized as renames so we update paths without re-extracting.
- Large external mutations (branch switch, dependency install) trigger a single prioritized reconcile, not a storm.

## 7. The edit-time fast path

Editing an open file doesn't go through the file watcher — it rides the edit pipeline directly:

```
edit ─► tree-sitter incremental reparse (the editor already did this for highlighting)
     ─► debounce (~120 ms after typing pauses)
     ─► extract FileDelta from the in-memory tree (no disk read)
     ─► apply to SQLite + resolve
     ─► invalidate dependent caches; push fresh diagnostics/symbols
```

Because the tree is already incrementally parsed for syntax highlighting, edit-time indexing reuses it — extraction is the only added cost, and it's bounded to one file.

## 8. Crash safety

Every applied `FileDelta` also appends to `change_log` (path, change, mtime, hash). The SQLite write and the log append are in one transaction. On an unclean shutdown, startup replays unapplied log entries and re-validates, so the index is never left half-written and never needs a full rebuild after a crash.

## 9. Cancellation & politeness

- Every index task carries a cancellation token. If you close a folder or the priority shifts, queued work for the abandoned scope is dropped.
- Foreground edits **preempt** background project indexing: the open-file fast path runs at higher priority so your current file is always current first.
- The background pool runs at low OS QoS; on battery or thermal pressure it throttles further. The machine stays usable.

## 10. Extensibility — language & fact extractors

Indexing is generic; *what gets extracted* is pluggable.

```rust
trait Extractor {
    fn languages(&self) -> &[Lang];
    fn version(&self) -> u32;   // bump → affected files marked stale
    fn extract(&self, file: &ParsedFile) -> FileDelta;
}
```

Core extractors: PHP ([05](./05-php-analysis-engine.md)), Laravel facts ([06](./06-laravel-intelligence.md)), Blade, JS/TS, Vue, SQL. Plugins register additional extractors via the SDK ([07](./07-plugin-sdk.md)) — e.g., a Filament or Livewire pack adds its own facts to the same store and they become navigable and searchable like everything else.

## 11. Observability

The "Photon Doctor" panel and status bar expose: files indexed / total, queue depth, last reconcile duration, unresolved-reference count, index size on disk, and memory used by index caches — so both users and support can see exactly what the indexer is doing.

→ Next: [05 — PHP Analysis Engine](./05-php-analysis-engine.md)
