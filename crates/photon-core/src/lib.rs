//! # photon-core
//!
//! The pure-logic core of Photon IDE: workspace model, SQLite index, PHP
//! analysis, Laravel intelligence, and Search Everywhere ranking. This crate
//! has **no GUI dependency** so it can be unit-tested in isolation — the Tauri
//! shell (`src-tauri`) is a thin layer of commands on top of it.
//!
//! See the architecture docs in `docs/` for the full design this implements.

pub mod db;
pub mod hover;
pub mod indexer;
pub mod infer;
pub mod inspect;
pub mod laravel;
pub mod php;
pub mod phpdoc;
pub mod refactor;
pub mod search;
pub mod selection;
pub mod types;
pub mod workspace;

pub use db::Index;
pub use indexer::Engine;
pub use selection::SelectionRange;
pub use types::{
    ArtifactInfo, Binding, ChangeSet, CompletionData, Diagnostic, EventListener, FileEntry,
    FileDiagnostics, JobInfo, KeyEntry, Location, MissingTranslation, ModelInfo, ProjectSummary,
    RefKind, Reference, RelationInfo, RootInfo, Route, SearchHit, Symbol, SymbolKind, TextEdit,
    UsageHit, UsagesResult,
};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PHP: &str = r#"<?php

namespace App\Http\Controllers;

use App\Models\User;

abstract class BaseController {}

interface Renderable {
    public function render(): string;
}

trait HasTimestamps {
    public $created_at;
}

enum Status: string {
    case Active = 'active';
    case Inactive = 'inactive';
}

class UserController extends BaseController implements Renderable
{
    const VERSION = '1.0';
    public int $count = 0;

    public function index(): string
    {
        return 'list';
    }

    public function show(int $id): string
    {
        return 'one';
    }

    public function render(): string
    {
        return 'rendered';
    }
}

function helper_fn(): void {}
"#;

    fn names_of(symbols: &[Symbol], kind: SymbolKind) -> Vec<String> {
        symbols
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.name.clone())
            .collect()
    }

    #[test]
    fn extracts_php_declarations() {
        let syms = php::extract_symbols("app/Http/Controllers/UserController.php", SAMPLE_PHP);

        assert!(names_of(&syms, SymbolKind::Class).contains(&"UserController".to_string()));
        assert!(names_of(&syms, SymbolKind::Class).contains(&"BaseController".to_string()));
        assert!(names_of(&syms, SymbolKind::Interface).contains(&"Renderable".to_string()));
        assert!(names_of(&syms, SymbolKind::Trait).contains(&"HasTimestamps".to_string()));
        assert!(names_of(&syms, SymbolKind::Enum).contains(&"Status".to_string()));
        assert!(names_of(&syms, SymbolKind::Function).contains(&"helper_fn".to_string()));

        let methods = names_of(&syms, SymbolKind::Method);
        assert!(methods.contains(&"index".to_string()));
        assert!(methods.contains(&"show".to_string()));
        assert!(methods.contains(&"render".to_string()));
    }

    #[test]
    fn extracts_promoted_constructor_properties() {
        let src = r#"<?php
namespace App\Services;

class OrderService
{
    public function __construct(
        private UserRepository $users,
        protected readonly Mailer $mailer,
        public Logger $log,
        int $plain,
    ) {}

    public function handle(): void {}
}
"#;
        let syms = php::extract_symbols("app/Services/OrderService.php", src);
        let props = names_of(&syms, SymbolKind::Property);
        // Promoted params become real properties...
        assert!(props.contains(&"users".to_string()), "users promoted: {props:?}");
        assert!(props.contains(&"mailer".to_string()), "mailer promoted: {props:?}");
        assert!(props.contains(&"log".to_string()), "log promoted: {props:?}");
        // ...but a plain (non-visibility) param does not.
        assert!(!props.contains(&"plain".to_string()), "plain not a property: {props:?}");
        // Promoted properties belong to the class.
        let users = syms
            .iter()
            .find(|s| s.kind == SymbolKind::Property && s.name == "users")
            .unwrap();
        assert_eq!(users.container.as_deref(), Some("OrderService"));
        assert!(users.range_end > users.range_start);
    }

    #[test]
    fn methods_get_fully_qualified_names() {
        let syms = php::extract_symbols("x.php", SAMPLE_PHP);
        let index_method = syms
            .iter()
            .find(|s| s.kind == SymbolKind::Method && s.name == "index")
            .expect("index method present");
        assert_eq!(
            index_method.fqn.as_deref(),
            Some("App\\Http\\Controllers\\UserController::index")
        );
        assert_eq!(index_method.container.as_deref(), Some("UserController"));
    }

    #[test]
    fn class_is_namespaced() {
        let syms = php::extract_symbols("x.php", SAMPLE_PHP);
        let ctrl = syms
            .iter()
            .find(|s| s.kind == SymbolKind::Class && s.name == "UserController")
            .unwrap();
        assert_eq!(
            ctrl.fqn.as_deref(),
            Some("App\\Http\\Controllers\\UserController")
        );
        // v2 body-range index: the class spans a real byte range.
        assert!(ctrl.range_end > ctrl.range_start);
        let index_method = syms
            .iter()
            .find(|s| s.kind == SymbolKind::Method && s.name == "index")
            .unwrap();
        assert!(index_method.range_end > index_method.range_start);
    }

    const SAMPLE_ROUTES: &str = r#"<?php

use App\Http\Controllers\UserController;
use Illuminate\Support\Facades\Route;

Route::get('/', function () {
    return view('welcome');
});

Route::get('/users', [UserController::class, 'index'])->name('users.index');
Route::post('/users', [UserController::class, 'store'])->name('users.store');
Route::get('/legacy', 'LegacyController@handle');
Route::match(['get', 'post'], '/multi', [UserController::class, 'multi']);
"#;

    #[test]
    fn extracts_routes() {
        let routes = laravel::extract_routes("routes/web.php", SAMPLE_ROUTES);
        assert_eq!(routes.len(), 5);

        let users_index = routes.iter().find(|r| r.uri == "/users" && r.method == "GET").unwrap();
        assert_eq!(users_index.name.as_deref(), Some("users.index"));
        assert_eq!(users_index.action.as_deref(), Some("UserController@index"));

        let root = routes.iter().find(|r| r.uri == "/").unwrap();
        assert_eq!(root.action.as_deref(), Some("Closure"));

        let legacy = routes.iter().find(|r| r.uri == "/legacy").unwrap();
        assert_eq!(legacy.action.as_deref(), Some("LegacyController@handle"));

        let multi = routes.iter().find(|r| r.uri == "/multi").unwrap();
        assert_eq!(multi.method, "GET|POST");
    }

    #[test]
    fn index_roundtrip_and_search() {
        let index = Index::open(":memory:").unwrap();
        let mut idx = index;
        // store sample symbols + routes
        let syms = php::extract_symbols("app/Http/Controllers/UserController.php", SAMPLE_PHP);
        idx.replace_symbols_for_file("app/Http/Controllers/UserController.php", &syms)
            .unwrap();
        let routes = laravel::extract_routes("routes/web.php", SAMPLE_ROUTES);
        idx.replace_routes_for_file("routes/web.php", &routes).unwrap();
        idx.insert_files(&[FileEntry {
            path: "app/Http/Controllers/UserController.php".into(),
            lang: "php".into(),
            size: SAMPLE_PHP.len() as u64,
            is_vendor: false,
            mtime: 0,
        }])
        .unwrap();

        // exact symbol lookup
        let found = idx.find_symbol("UserController").unwrap();
        assert!(!found.is_empty());

        // search everywhere finds the controller by fuzzy query
        let hits = search::search_everywhere(&idx, "usercon", 20);
        assert!(hits.iter().any(|h| h.label == "UserController"));

        // search finds a route
        let route_hits = search::search_everywhere(&idx, "users.index", 20);
        assert!(route_hits.iter().any(|h| h.category == "route"));
    }

    #[test]
    fn persistence_reconcile_primitives() {
        let mut idx = Index::open(":memory:").unwrap();
        // Schema-version gate: fresh DB is fine, drift wipes.
        idx.ensure_schema_version(1).unwrap();
        idx.insert_files(&[
            FileEntry { path: "app/A.php".into(), lang: "php".into(), size: 1, is_vendor: false, mtime: 100 },
            FileEntry { path: "app/B.php".into(), lang: "php".into(), size: 1, is_vendor: false, mtime: 200 },
        ])
        .unwrap();

        let stored = idx.file_mtimes_with_prefix("app").unwrap();
        assert_eq!(stored.get("app/A.php"), Some(&100));
        assert_eq!(stored.get("app/B.php"), Some(&200));

        // upsert updates mtime in place.
        idx.upsert_file(&FileEntry {
            path: "app/A.php".into(),
            lang: "php".into(),
            size: 2,
            is_vendor: false,
            mtime: 999,
        })
        .unwrap();
        assert_eq!(idx.file_mtimes_with_prefix("app").unwrap().get("app/A.php"), Some(&999));

        // delete removes the file row entirely.
        idx.delete_file_rows("app/B.php").unwrap();
        let after = idx.file_mtimes_with_prefix("app").unwrap();
        assert!(after.contains_key("app/A.php"));
        assert!(!after.contains_key("app/B.php"));

        // A schema-version change wipes prior data (forces re-index).
        idx.ensure_schema_version(2).unwrap();
        assert!(idx.file_mtimes_with_prefix("app").unwrap().is_empty());
    }

    #[test]
    fn fuzzy_scoring_orders_prefix_first() {
        let exact = search::fuzzy_score("user", "user").unwrap();
        let prefix = search::fuzzy_score("user", "username").unwrap();
        let mid = search::fuzzy_score("user", "getuser").unwrap();
        let none = search::fuzzy_score("xyz", "user");
        assert!(exact > prefix);
        assert!(prefix > mid);
        assert!(none.is_none());
    }

    // ---- v1: references, rename, Laravel depth ----

    const REF_SOURCE: &str = r#"<?php
namespace App\Services;

use App\Models\User;

class UserService
{
    public function make(): User
    {
        $u = new User();
        return User::create([]);
    }
}
"#;

    #[test]
    fn extracts_references_to_user() {
        let refs = php::extract_references("app/Services/UserService.php", REF_SOURCE);
        let user_refs: Vec<_> = refs.iter().filter(|r| r.name == "User").collect();
        // use import + return type + `new User` + static `User::create`
        assert!(user_refs.len() >= 3, "got {} User refs", user_refs.len());
        assert!(user_refs.iter().any(|r| r.kind == RefKind::Import));
        assert!(user_refs.iter().any(|r| r.kind == RefKind::StaticRef));
    }

    #[test]
    fn rename_plans_changeset_across_files() {
        let mut idx = Index::open(":memory:").unwrap();
        // Define class User in a model file...
        let model_src = "<?php\nnamespace App\\Models;\nclass User extends Model {}\n";
        idx.replace_symbols_for_file("app/Models/User.php", &php::extract_symbols("app/Models/User.php", model_src)).unwrap();
        idx.replace_refs_for_file("app/Models/User.php", &php::extract_references("app/Models/User.php", model_src)).unwrap();
        // ...and reference it from a service.
        idx.replace_refs_for_file("app/Services/UserService.php", &php::extract_references("app/Services/UserService.php", REF_SOURCE)).unwrap();

        let read = |f: &str| -> Option<String> {
            match f {
                "app/Models/User.php" => Some(model_src.to_string()),
                "app/Services/UserService.php" => Some(REF_SOURCE.to_string()),
                _ => None,
            }
        };
        let plan = refactor::plan_rename(&idx, "User", "Account", &read).unwrap();
        assert!(plan.files_affected >= 2, "expected edits in >=2 files");
        assert!(plan.edits.iter().any(|e| e.file.ends_with("User.php")));
        assert!(plan.edits.iter().any(|e| e.file.ends_with("UserService.php")));

        // Apply and confirm the definition got renamed.
        let applied = refactor::apply_changeset(&plan, None, &read).unwrap();
        let user_file = applied.iter().find(|(f, _)| f.ends_with("Models/User.php")).unwrap();
        assert!(user_file.1.contains("class Account"));
    }

    const MODEL_SOURCE: &str = r#"<?php
namespace App\Models;

use Illuminate\Database\Eloquent\Model;

class Post extends Model
{
    protected $table = 'posts';
    protected $fillable = ['title', 'body'];

    public function author()
    {
        return $this->belongsTo(User::class);
    }

    public function comments()
    {
        return $this->hasMany(Comment::class);
    }
}
"#;

    #[test]
    fn extracts_model_with_relations() {
        let models = laravel::extract_models("app/Models/Post.php", MODEL_SOURCE, Some("App\\Models"));
        assert_eq!(models.len(), 1);
        let post = &models[0];
        assert_eq!(post.name, "Post");
        assert_eq!(post.table.as_deref(), Some("posts"));
        assert_eq!(post.fqn.as_deref(), Some("App\\Models\\Post"));
        assert!(post.fillable.contains(&"title".to_string()));

        let author = post.relations.iter().find(|r| r.method == "author").unwrap();
        assert_eq!(author.rel_type, "belongsTo");
        assert_eq!(author.related.as_deref(), Some("User"));
        let comments = post.relations.iter().find(|r| r.method == "comments").unwrap();
        assert_eq!(comments.rel_type, "hasMany");
        assert_eq!(comments.related.as_deref(), Some("Comment"));
    }

    const CONFIG_SOURCE: &str = r#"<?php
return [
    'name' => env('APP_NAME', 'Laravel'),
    'providers' => [
        'stripe' => [
            'key' => env('STRIPE_KEY'),
        ],
    ],
];
"#;

    #[test]
    fn extracts_nested_config_keys() {
        let keys = laravel::extract_config_keys("config/app.php", CONFIG_SOURCE, "app");
        let paths: Vec<&str> = keys.iter().map(|k| k.key.as_str()).collect();
        assert!(paths.contains(&"app.name"));
        assert!(paths.contains(&"app.providers.stripe.key"));
    }

    #[test]
    fn extracts_container_bindings() {
        let src = r#"<?php
class AppServiceProvider {
    public function register() {
        $this->app->singleton(PaymentGateway::class, StripeGateway::class);
        $this->app->bind('mailer', function () { return new Mailer(); });
    }
}
"#;
        let b = laravel::extract_bindings("app/Providers/AppServiceProvider.php", src);
        let pg = b.iter().find(|x| x.abstract_name == "PaymentGateway").unwrap();
        assert_eq!(pg.kind, "singleton");
        assert_eq!(pg.concrete.as_deref(), Some("StripeGateway"));
        let mailer = b.iter().find(|x| x.abstract_name == "mailer").unwrap();
        assert_eq!(mailer.concrete.as_deref(), Some("Closure"));
    }

    #[test]
    fn extracts_event_listeners() {
        let src = r#"<?php
class EventServiceProvider {
    protected $listen = [
        OrderShipped::class => [
            SendShipmentNotification::class,
            UpdateInventory::class,
        ],
        UserRegistered::class => [SendWelcomeEmail::class],
    ];
}
"#;
        let ev = laravel::extract_events("app/Providers/EventServiceProvider.php", src);
        let shipped: Vec<_> = ev.iter().filter(|e| e.event == "OrderShipped").map(|e| e.listener.clone()).collect();
        assert!(shipped.contains(&"SendShipmentNotification".to_string()));
        assert!(shipped.contains(&"UpdateInventory".to_string()));
        assert!(ev.iter().any(|e| e.event == "UserRegistered" && e.listener == "SendWelcomeEmail"));
    }

    #[test]
    fn extracts_queued_job() {
        let src = "<?php\nnamespace App\\Jobs;\nuse Illuminate\\Contracts\\Queue\\ShouldQueue;\nclass ProcessPodcast implements ShouldQueue { public function handle() {} }\n";
        let jobs = laravel::extract_jobs("app/Jobs/ProcessPodcast.php", src);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "ProcessPodcast");
        assert!(jobs[0].queued);
    }

    #[test]
    fn extracts_member_types() {
        let src = r#"<?php
namespace App;
class Repo {
    private UserService $service;
    public function find(int $id): User { }
    public function self_(): self { return $this; }
}
"#;
        let mt = php::extract_member_types("x.php", src);
        let find = |c: &str, m: &str| mt.iter().find(|(cc, mm, _, _)| cc == c && mm == m).map(|(_, _, _, t)| t.clone());
        assert_eq!(find("Repo", "find").as_deref(), Some("User"));
        assert_eq!(find("Repo", "service").as_deref(), Some("UserService"));
        assert_eq!(find("Repo", "self_").as_deref(), Some("self"));
    }

    #[test]
    fn extracts_type_relations() {
        let src = r#"<?php
namespace App;
interface Renderable {}
class Base {}
class Widget extends Base implements Renderable {
    use HasTimestamps;
}
"#;
        let rels = php::extract_type_relations("x.php", src);
        let has = |s: &str, d: &str, r: &str| rels.iter().any(|(a, b, c)| a == s && b == d && c == r);
        assert!(has("Widget", "Base", "extends"));
        assert!(has("Widget", "Renderable", "implements"));
        assert!(has("Widget", "HasTimestamps", "uses"));
    }

    #[test]
    fn extracts_migration_columns() {
        let src = r#"<?php
return new class extends Migration {
    public function up(): void {
        Schema::create('users', function (Blueprint $table) {
            $table->id();
            $table->string('name');
            $table->string('email')->unique();
            $table->timestamp('email_verified_at')->nullable();
            $table->rememberToken();
            $table->timestamps();
        });
    }
};
"#;
        let cols = laravel::extract_migration_columns(src);
        let has = |t: &str, c: &str| cols.iter().any(|(tt, cc, _)| tt == t && cc == c);
        assert!(has("users", "id"));
        assert!(has("users", "name"));
        assert!(has("users", "email"));
        assert!(has("users", "email_verified_at"));
        assert!(has("users", "remember_token"));
        assert!(has("users", "created_at"));
        assert!(has("users", "updated_at"));
    }

    #[test]
    fn detects_missing_translations() {
        let mut idx = Index::open(":memory:").unwrap();
        idx.replace_translations_for_file(
            "lang/en/auth.php",
            &[
                KeyEntry { key: "auth.failed".into(), locale: "en".into(), file: "lang/en/auth.php".into(), line: 2 },
                KeyEntry { key: "auth.throttle".into(), locale: "en".into(), file: "lang/en/auth.php".into(), line: 3 },
            ],
        ).unwrap();
        idx.replace_translations_for_file(
            "lang/fr/auth.php",
            &[KeyEntry { key: "auth.failed".into(), locale: "fr".into(), file: "lang/fr/auth.php".into(), line: 2 }],
        ).unwrap();

        let missing = idx.missing_translations().unwrap();
        let throttle = missing.iter().find(|m| m.key == "auth.throttle").unwrap();
        assert!(throttle.missing_in.contains(&"fr".to_string()));
    }
}
