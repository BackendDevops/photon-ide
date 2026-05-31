# 05 — PHP Analysis Engine

To match PhpStorm, Photon needs genuine semantic understanding of PHP — not regexes. This document specifies the bespoke PHP engine: parsing, name resolution, the type system, and how it handles the hard parts (generics via PHPDoc, traits, magic methods, attributes, enums, dynamic return types, anonymous classes).

## 1. Why a custom engine (and where LSP fits)

Off-the-shelf LSP servers (Intelephense, Phpactor) are good but: (a) we can't control their performance/memory to hit our targets, (b) Laravel intelligence needs deep, custom hooks into resolution, and (c) we want one engine feeding both the index and on-demand queries. So Photon ships its **own engine** as the primary, with the `Php` contract ([02](./02-module-design.md)) allowing an external LSP as a **fallback/supplement** (e.g., for a PHP construct we haven't covered yet, or for users who prefer it). The engine and an LSP can even run side by side, with the engine authoritative for Laravel-aware features.

## 2. Pipeline

```
source ─► tree-sitter (incremental CST) ─► lowering ─► AST + scopes
      ─► name resolution (namespaces, use, autoload) ─► resolved AST
      ─► type inference (declared + PHPDoc + flow) ─► SemanticModel
      ─► index extraction (FileDelta)  +  on-demand queries (hover, completion)
```

- **tree-sitter** gives a fast, error-tolerant concrete syntax tree that updates incrementally on edit (shared with the editor's syntax highlighting — parse once, use twice).
- **Lowering** turns the CST into a compact AST with scope information.
- The **SemanticModel** is the queryable result: for any position, "what symbol is this, what's its type, what are its members."

Error tolerance matters: developers spend most of their time with *incomplete, invalid* code. The engine must produce useful results for a half-typed expression, which is exactly what tree-sitter's error recovery enables.

## 3. Name resolution

PHP's name resolution is non-trivial and must be exact for navigation to be trustworthy.

- **Namespaces & `use`:** resolve unqualified, qualified, and fully-qualified names against the current namespace, `use` imports (class/function/const, with aliases), and group-use statements.
- **Autoload awareness:** parse `composer.json` PSR-4/PSR-0/classmap/files autoload rules to map FQNs ↔ file paths. This makes "go to class" work even before a file is opened and validates that a class lives where its namespace claims.
- **Resolution scopes:** a stack of namespace → imports → class (self/static/parent) → function/closure (with `use` captures) → block. Late static binding (`static::`) is tracked distinctly from `self::`.
- Results feed `symbols.fqn_id` and the resolution of `references`/`symbol_relations` in the index ([03](./03-database-schema.md), [04](./04-indexing-engine.md)).

## 4. Type system

A structural type lattice rich enough for modern PHP:

```
Type :=
  | scalar (int|float|string|bool|null|true|false)
  | array<K,V> | list<V> | non-empty-array | array-shape{...}
  | object(FQN)         // a class/interface instance
  | generic FQN<T...>   // e.g. Collection<int, User> (PHPDoc-driven)
  | callable(params): ret | Closure
  | iterable<K,V> | Generator<K,V,S,R>
  | union (A|B) | intersection (A&B)
  | nullable (?T)
  | self | static | parent | $this
  | enum(FQN) | enum-case
  | class-string<T> | literal-string | int-range
  | mixed | never | void
  | template T (generic parameter)
```

### Sources of type information (in precedence order)
1. **Native type declarations** — param types, return types, property types, `readonly`, union/intersection, `never`/`void` (highest confidence).
2. **PHPDoc** — `@param`, `@return`, `@var`, `@property`, `@method`, `@template`, `@extends`, `@implements`, `@phpstan-*`/`@psalm-*` extended syntax for generics and array shapes. PHPDoc *refines* native types (e.g. native `array`, PHPDoc `array<int, User>` → `list<User>`).
3. **Flow inference** — local dataflow: assignment, narrowing via `instanceof`/`is_*`/`null` checks, `match`/`switch` narrowing, early returns. Confidence stored per `symbol_types.confidence`.
4. **Dynamic/heuristic** — for genuinely dynamic constructs, a best-effort type with low confidence, surfaced honestly (no false certainty).

### Generics
PHP has no runtime generics, so generics are **PHPDoc-driven** (`@template`, `@template-covariant`, `@extends Collection<int,User>`, `@return T`). The engine implements a constraint solver good enough for the common cases that make Laravel pleasant: `Collection<User>::first()` → `User`, `Builder<User>::get()` → `Collection<int,User>`, mapped/filtered collections preserving element types. This is the same modeling PHPStan/Psalm use, so existing well-annotated code "just works."

## 5. The hard parts (explicit coverage)

**Traits.** Resolve `use TraitA, TraitB;` including conflict resolution (`insteadof`, `as` aliasing/visibility change). Trait members are flattened into the using class's member set with provenance tracked (so "go to definition" jumps to the trait, and rename across trait users is correct).

**Interfaces & abstract classes.** Full implements/extends graph (stored in `symbol_relations`). Abstract method declarations participate in completion and in "find implementations." Multiple interface inheritance handled.

**Magic methods.** `__get`/`__set`/`__call`/`__callStatic`/`__invoke` are modeled. Where the class documents them via `@method`/`@property` PHPDoc tags, those become real, navigable, completable members. Where not, the engine knows access is dynamic and degrades gracefully (offers but marks uncertain) rather than reporting false "undefined" errors. This is critical for Eloquent and many libraries.

**Magic properties.** `@property`, `@property-read`, `@property-write` tags create virtual properties with types — heavily used by Eloquent models and the basis for column completion (combined with DB schema in [06](./06-laravel-intelligence.md)).

**Attributes (PHP 8+).** Parsed as first-class (`#[Route(...)]`, `#[ORM\Column]`, custom). Stored in `symbol_attributes`. Attribute targets, arguments, and the attribute class are all resolved — enabling attribute-based routing, validation, and DI to be understood.

**Enums (PHP 8.1+).** Pure and backed enums, cases, enum methods, interface implementation by enums, and `from()/tryFrom()` return typing.

**Anonymous classes.** Given synthetic stable ids, their member set and the interfaces/parent they extend are modeled so `new class extends X {}` participates in type inference.

**Dynamic return types.** Beyond generics: conditional return types (`@return ($x is true ? A : B)`), `static` returns (fluent builders return the concrete subclass), and configured "dynamic return type" providers (the same mechanism Laravel intelligence uses to teach the engine that `app(Foo::class)` returns `Foo`, `$collection->first()` returns the element type, etc.). Plugins can register such providers via the SDK.

**First-class callable & closures.** `strlen(...)`, arrow fns, `Closure::bind`, captured `use` variables typed.

## 6. What the engine produces (features it powers)

- **Completion:** members, namespaced names, parameters, array-shape keys, enum cases, magic members from PHPDoc — ranked by type relevance.
- **Hover & signatures:** rendered type, PHPDoc summary, parameter hints.
- **Semantic highlighting:** tokens colored by resolved meaning (a property vs a method vs a constant), not just lexical class.
- **Diagnostics:** undefined symbol, wrong argument count/type, accessing private members, unreachable code, missing return — with confidence gating so dynamic code isn't spammed with false positives.
- **Inlay hints:** inferred types, parameter names at call sites.
- **Navigation & refactoring** ([02](./02-module-design.md)): the resolved graph is what makes rename/find-usages correct.

## 7. Performance & memory

- **Parse once, incrementally.** The CST is reused across edits; only changed subtrees re-lower. Closed files drop their trees.
- **On-demand vs indexed.** Cheap, whole-project facts (symbol locations, signatures, relations) are precomputed into the index. Expensive, deep inference (full flow typing of a method body) is computed lazily for the file/expression in view and cached by revision.
- **Bounded caches.** SemanticModels for open files are cached; an LRU bounds memory and evicts cold models.
- **Interning.** Names/types are interned; the type lattice uses compact representations.

## 8. Configuration & standards

- Reads `composer.json` (autoload, php version constraint, dependencies) and respects the project's PHP version for which syntax/features are valid.
- Honors PHPStan/Psalm-style PHPDoc so codebases already annotated for static analysis get maximum intelligence for free.
- Per-project overrides (PHP version, baseline, stub paths). Bundled PHP core + common extension stubs ensure built-in functions/classes are fully typed.

## 9. Validation strategy

Correctness is verified against: (a) a corpus of real open-source Laravel/PHP projects with known navigation/typing expectations, (b) the PHPStan/Psalm test fixtures for type inference parity, and (c) golden-file tests on the symbol/reference output so extractor changes are reviewable as diffs.

→ Next: [06 — Laravel Intelligence Engine](./06-laravel-intelligence.md)
