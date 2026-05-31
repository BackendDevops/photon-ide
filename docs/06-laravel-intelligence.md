# 06 — Laravel Intelligence Engine

This is the flagship. Generic PHP intelligence ([05](./05-php-analysis-engine.md)) makes Photon a competent PHP IDE; the Laravel engine is what makes it *the* IDE Laravel developers choose. The goal is to rival — and in performance, beat — Laravel Idea.

## 1. Approach: understand the framework as a domain, not as text

Laravel is a convention-heavy framework with lots of "magic" (facades, dynamic Eloquent properties, container resolution, string-based route/view/config references). A regex-and-hope approach is fragile. Photon instead builds a **Laravel project model**: it discovers the framework version, reads the conventional structure, parses the relevant source, and (optionally) augments with reflection from a booted app. These facts populate the `laravel_*` tables ([03](./03-database-schema.md)) and are wired into resolution, completion, navigation, and Search Everywhere.

### Two sources of truth, reconciled
1. **Static analysis (always on):** parse routes files, models, providers, Blade templates, config, lang files, jobs, events. Fast, works offline, no app boot, safe.
2. **Runtime reflection (optional, opt-in):** run a small artisan-based introspection command (à la `route:list --json`, plus a Photon helper command shipped as a dev dependency or invoked ad hoc) to capture facts that are hard to get statically — fully resolved route lists, container bindings registered at runtime, package-discovered providers. Results are merged into the model and clearly attributed. This is gated, sandboxed (runs the user's own app in their environment), and cached.

Static is the floor; runtime reflection is the precision boost. Most features work great on static alone.

## 2. Framework detection & versioning

On project open the engine detects Laravel by `composer.json` (`laravel/framework`) and reads the version constraint, locating `routes/`, `app/`, `config/`, `resources/views/`, `lang/`, `database/`. Conventions and APIs differ across major versions (e.g. routing/middleware changes, `lang/` location, Folio/Volt), so the engine carries **version-aware rule sets**. Non-standard layouts are supported via `project_meta` overrides. Related ecosystems (Livewire, Filament, Inertia, Folio, Volt, Nova) are recognized and handled by bundled or marketplace packs built on the same SDK ([07](./07-plugin-sdk.md)).

## 3. Routes

**Discovery.** Parse `routes/*.php` and attribute-based routes (`#[Route]`), capturing method, URI, name, action (controller@method or closure), middleware, and groups (prefix/name/domain/middleware inheritance). Store in `laravel_routes`.

**Features:**
- **Navigation:** from `route('users.show')` / `Route::get(...)` / a Blade `@route`/`route()` call → jump to the controller action. From a controller action → find the route(s) that map to it.
- **Completion:** route names in `route()`, `redirect()->route()`, `URL::route()`; URI parameters; middleware names.
- **Usage search:** "where is this route referenced" across PHP, Blade, and JS (e.g. Ziggy).
- **Validation:** unknown route name, missing required route parameter, parameter name mismatch — surfaced as diagnostics.
- Routes appear as a category in **Search Everywhere** ("⇧⇧ users.show").

## 4. Eloquent

The richest area, because Eloquent leans hard on PHP magic.

- **Models:** detect classes extending `Model`; capture `$table` (or infer from class name), `$fillable`, `$guarded`, `$casts`, `$primaryKey`, `$connection`, timestamps. Store in `laravel_models`.
- **Relationship inference:** detect relation methods (`hasOne/hasMany/belongsTo/belongsToMany/hasManyThrough/morphTo/morphMany/...`), infer the related model from the method body (the `::class` argument or convention), pivot tables, and keys. Store in `laravel_relations`. This powers: navigate from `$user->posts` to the `Post` model, completion of relation methods, and typing `$user->posts` as `Collection<Post>` / `$user->profile` as `Profile`.
- **Dynamic attributes / columns:** combine `@property` PHPDoc, `$casts`, and — when a DB connection is attached ([02](./02-module-design.md) `database`) — the **real table schema** to offer column completion and flag typos (`$user->emale` → unknown column). The `laravel_models.db_object_id` link makes this exact.
- **Query builder awareness:** model the fluent builder so `User::query()->where(...)->first()` types correctly (`User`), `->get()` → `Collection<User>`, `->paginate()` → `LengthAwarePaginator<User>`. `where`/`orderBy` column arguments get completion from the model's columns. This uses the dynamic-return-type provider mechanism from [05](./05-php-analysis-engine.md).
- **Scopes:** detect `scopeActive()` → usable as `->active()` with correct typing; navigate from the call to the scope method.
- **Factories & seeders:** link `UserFactory` ↔ `User`; navigate model ↔ factory; complete factory states; understand `definition()` return shape. Seeders discovered and runnable as actions.
- **Accessors/mutators/casts:** modern `Attribute`-based and legacy `getXAttribute` accessors modeled as virtual typed properties.

## 5. Service container

- **Binding navigation:** parse `bind`/`singleton`/`instance`/`alias` in service providers (and runtime-reflected bindings when enabled). Store in `laravel_bindings`.
- **Service resolution:** `app(Foo::class)`, `App::make(Foo::class)`, `resolve(Foo::class)`, and **constructor injection** typed to the concrete (or the interface) — `app(PaymentGateway::class)` navigates to the bound concrete implementation.
- **Dependency tracing:** "what is bound to this interface", "where is this binding registered", "what depends on this service" — a small dependency graph view.
- Facades resolved to their underlying class (`Cache::` → `CacheManager`/`Repository` methods) so facade calls complete and navigate correctly.

## 6. Blade

- **Component navigation:** `<x-button>` / `<x-forms.input>` → the component class and/or view; `@component`, `@include`, `@extends`, `@livewire` targets navigable. Anonymous and class-based components both handled. Stored in `laravel_views`.
- **Prop completion & validation:** complete a component's `@props([...])` / public properties as `<x-button :variant=...>` attributes; flag unknown props.
- **Slot awareness:** named slots (`<x-slot:title>`) understood and completed.
- **Directive intelligence:** built-in directives (`@if/@foreach/@auth/@can/@error/@push/@stack/...`) get completion, snippet expansion, and matching-tag highlighting; custom directives registered in the app are discovered and added.
- **Cross-language typing inside Blade:** `@php` blocks and `{{ $var }}` expressions are analyzed with the PHP engine, and variables passed from a controller (`view('x', ['user' => $user])`) are typed inside the template where statically resolvable.
- **View name navigation/completion:** `view('users.index')` → the Blade file; completion of dotted view names.

## 7. Config

- Index every key path in `config/*.php` into `laravel_config` (`'services.stripe.key'`).
- **Navigation:** `config('services.stripe.key')` → the exact array key in the config file.
- **Completion:** config key paths inside `config()`/`Config::get()`.
- **Validation:** unknown config key warning. `env()` keys cross-checked against `.env`/`.env.example` with "missing env var" hints.

## 8. Localization

- Index translation keys across `lang/{locale}/*.php` and JSON lang files into `laravel_translations`.
- **Navigation:** `__('auth.failed')` / `@lang` / `trans()` → the key definition.
- **Completion:** translation keys.
- **Missing translation detection:** key used but absent in a locale → diagnostic; a panel listing missing/orphan keys per locale.

## 9. Events & listeners

- Discover event→listener wiring from `EventServiceProvider::$listen`, attribute-based listeners, subscriber classes, and auto-discovery. Store in `laravel_events`.
- **Navigation:** from `event(OrderShipped::class)` / `OrderShipped::dispatch()` → all listeners; from a listener → the event(s) it handles.
- **Listener discovery:** "who listens to this event."

## 10. Queues & jobs

- Detect `ShouldQueue` jobs, their `$queue`/`$connection`, and dispatch sites (`Job::dispatch()`, `dispatch(new Job)`, `->onQueue()`). Store in `laravel_jobs`.
- **Navigation:** dispatch site ↔ job class; trace a job's queue/connection.
- **Queue tracing:** a view of jobs grouped by queue/connection; navigate to handlers.

## 11. Artisan & tooling integration

- Discover artisan commands (built-in + app + package) and surface them as **Search Everywhere actions** and a command runner (`artisan migrate`, `make:*`, `queue:work`). `make:*` generators integrate with the file tree.
- Migrations understood enough to correlate schema changes with models; "run migration", "rollback" as actions.
- Composer scripts and npm scripts likewise surfaced as actions.

## 12. How it plugs into the rest of the IDE

- **Resolution/typing:** Laravel facts register dynamic-return-type and virtual-member providers with the PHP engine ([05](./05-php-analysis-engine.md)), so Eloquent/builder/container magic types correctly everywhere.
- **Index:** all facts live in `laravel_*` tables ([03](./03-database-schema.md)), updated incrementally per file ([04](./04-indexing-engine.md)) — edit a model, its relations/columns refresh in milliseconds.
- **Search Everywhere & navigation:** routes, models, views, config keys, and translations are first-class search categories and navigation targets ([02](./02-module-design.md)).
- **Refactoring:** rename a route name, a config key, a translation key, or a model property and references update across PHP **and** Blade **and** (where applicable) JS.
- **AI:** the Laravel model is prime context for the AI subsystem ([10](./10-ai-subsystem.md)) — the agent knows your routes, models, and bindings.

## 13. Performance posture

Everything here is incremental and file-scoped. Editing a model re-extracts that one file's Laravel facts and re-resolves only dependent relations — no project rescan. Runtime reflection is cached and only re-run on explicit request or when its inputs (providers, composer) change. The flagship features therefore cost essentially nothing at steady state, which is how Photon can out-perform JVM-based Laravel tooling while matching its depth.

→ Next: [07 — Plugin SDK Specification](./07-plugin-sdk.md)
