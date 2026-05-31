//! File templates (docs/06 §Artisan & tooling; docs/12 generators).
//!
//! Built-in PHP/Laravel templates plus user templates from
//! `.photon/templates/*.json` and extension-contributed templates. Each
//! template has a path pattern and body with `{{var}}` placeholders.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateField {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub label: String,
    pub category: String,
    /// Output path pattern, relative to project root, with `{{var}}` placeholders.
    pub filename: String,
    pub body: String,
    #[serde(default)]
    pub fields: Vec<TemplateField>,
    #[serde(default = "builtin_source")]
    pub source: String,
}

fn builtin_source() -> String {
    "builtin".into()
}

fn name_field() -> Vec<TemplateField> {
    vec![TemplateField {
        key: "name".into(),
        label: "Name".into(),
        default: String::new(),
    }]
}

/// The bundled templates.
pub fn builtins() -> Vec<Template> {
    let t = |id: &str, label: &str, category: &str, filename: &str, body: &str| Template {
        id: id.into(),
        label: label.into(),
        category: category.into(),
        filename: filename.into(),
        body: body.into(),
        fields: name_field(),
        source: "builtin".into(),
    };

    vec![
        t(
            "php-class",
            "PHP Class",
            "PHP",
            "app/{{name}}.php",
            "<?php\n\nnamespace App;\n\nclass {{name}}\n{\n    //\n}\n",
        ),
        t(
            "php-interface",
            "PHP Interface",
            "PHP",
            "app/Contracts/{{name}}.php",
            "<?php\n\nnamespace App\\Contracts;\n\ninterface {{name}}\n{\n    //\n}\n",
        ),
        t(
            "controller",
            "Laravel Controller",
            "Laravel",
            "app/Http/Controllers/{{name}}.php",
            "<?php\n\nnamespace App\\Http\\Controllers;\n\nuse Illuminate\\Http\\Request;\n\nclass {{name}} extends Controller\n{\n    public function index(Request $request)\n    {\n        //\n    }\n}\n",
        ),
        t(
            "model",
            "Eloquent Model",
            "Laravel",
            "app/Models/{{name}}.php",
            "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\nclass {{name}} extends Model\n{\n    protected $fillable = [];\n}\n",
        ),
        t(
            "migration",
            "Migration",
            "Laravel",
            "database/migrations/{{timestamp}}_{{name}}.php",
            "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{\n    public function up(): void\n    {\n        Schema::create('{{name}}', function (Blueprint $table) {\n            $table->id();\n            $table->timestamps();\n        });\n    }\n\n    public function down(): void\n    {\n        Schema::dropIfExists('{{name}}');\n    }\n};\n",
        ),
        t(
            "request",
            "Form Request",
            "Laravel",
            "app/Http/Requests/{{name}}.php",
            "<?php\n\nnamespace App\\Http\\Requests;\n\nuse Illuminate\\Foundation\\Http\\FormRequest;\n\nclass {{name}} extends FormRequest\n{\n    public function authorize(): bool\n    {\n        return true;\n    }\n\n    public function rules(): array\n    {\n        return [];\n    }\n}\n",
        ),
        t(
            "middleware",
            "Middleware",
            "Laravel",
            "app/Http/Middleware/{{name}}.php",
            "<?php\n\nnamespace App\\Http\\Middleware;\n\nuse Closure;\nuse Illuminate\\Http\\Request;\n\nclass {{name}}\n{\n    public function handle(Request $request, Closure $next)\n    {\n        return $next($request);\n    }\n}\n",
        ),
        t(
            "blade-component",
            "Blade Component (anonymous)",
            "Blade",
            "resources/views/components/{{name}}.blade.php",
            "@props([])\n\n<div {{ '{{' }} $attributes {{ '}}' }}>\n    {{ '{{' }} $slot {{ '}}' }}\n</div>\n",
        ),
        t(
            "blade-view",
            "Blade View",
            "Blade",
            "resources/views/{{name}}.blade.php",
            "<x-app-layout>\n    <div class=\"container\">\n        {{-- {{name}} --}}\n    </div>\n</x-app-layout>\n",
        ),
        t(
            "pest-test",
            "Pest Test",
            "Testing",
            "tests/Feature/{{name}}Test.php",
            "<?php\n\nit('works', function () {\n    expect(true)->toBeTrue();\n});\n",
        ),
        t(
            "resource",
            "API Resource",
            "Laravel",
            "app/Http/Resources/{{name}}.php",
            "<?php\n\nnamespace App\\Http\\Resources;\n\nuse Illuminate\\Http\\Request;\nuse Illuminate\\Http\\Resources\\Json\\JsonResource;\n\nclass {{name}} extends JsonResource\n{\n    public function toArray(Request $request): array\n    {\n        return parent::toArray($request);\n    }\n}\n",
        ),
        t(
            "notification",
            "Notification",
            "Laravel",
            "app/Notifications/{{name}}.php",
            "<?php\n\nnamespace App\\Notifications;\n\nuse Illuminate\\Bus\\Queueable;\nuse Illuminate\\Notifications\\Notification;\nuse Illuminate\\Notifications\\Messages\\MailMessage;\n\nclass {{name}} extends Notification\n{\n    use Queueable;\n\n    public function via(object $notifiable): array\n    {\n        return ['mail'];\n    }\n\n    public function toMail(object $notifiable): MailMessage\n    {\n        return (new MailMessage)->line('...');\n    }\n}\n",
        ),
        t(
            "mailable",
            "Mailable",
            "Laravel",
            "app/Mail/{{name}}.php",
            "<?php\n\nnamespace App\\Mail;\n\nuse Illuminate\\Bus\\Queueable;\nuse Illuminate\\Mail\\Mailable;\nuse Illuminate\\Mail\\Mailables\\Content;\nuse Illuminate\\Mail\\Mailables\\Envelope;\nuse Illuminate\\Queue\\SerializesModels;\n\nclass {{name}} extends Mailable\n{\n    use Queueable, SerializesModels;\n\n    public function envelope(): Envelope\n    {\n        return new Envelope(subject: '{{name}}');\n    }\n\n    public function content(): Content\n    {\n        return new Content(view: 'view.name');\n    }\n}\n",
        ),
        t(
            "policy",
            "Policy",
            "Laravel",
            "app/Policies/{{name}}.php",
            "<?php\n\nnamespace App\\Policies;\n\nuse App\\Models\\User;\n\nclass {{name}}\n{\n    public function viewAny(User $user): bool\n    {\n        return true;\n    }\n}\n",
        ),
        t(
            "job",
            "Queued Job",
            "Laravel",
            "app/Jobs/{{name}}.php",
            "<?php\n\nnamespace App\\Jobs;\n\nuse Illuminate\\Bus\\Queueable;\nuse Illuminate\\Contracts\\Queue\\ShouldQueue;\nuse Illuminate\\Foundation\\Bus\\Dispatchable;\nuse Illuminate\\Queue\\InteractsWithQueue;\nuse Illuminate\\Queue\\SerializesModels;\n\nclass {{name}} implements ShouldQueue\n{\n    use Dispatchable, InteractsWithQueue, Queueable, SerializesModels;\n\n    public function handle(): void\n    {\n        //\n    }\n}\n",
        ),
        t(
            "event",
            "Event",
            "Laravel",
            "app/Events/{{name}}.php",
            "<?php\n\nnamespace App\\Events;\n\nuse Illuminate\\Foundation\\Events\\Dispatchable;\nuse Illuminate\\Queue\\SerializesModels;\n\nclass {{name}}\n{\n    use Dispatchable, SerializesModels;\n\n    public function __construct()\n    {\n        //\n    }\n}\n",
        ),
        t(
            "listener",
            "Listener",
            "Laravel",
            "app/Listeners/{{name}}.php",
            "<?php\n\nnamespace App\\Listeners;\n\nclass {{name}}\n{\n    public function handle(object $event): void\n    {\n        //\n    }\n}\n",
        ),
        t(
            "observer",
            "Model Observer",
            "Laravel",
            "app/Observers/{{name}}.php",
            "<?php\n\nnamespace App\\Observers;\n\nclass {{name}}\n{\n    public function created($model): void\n    {\n        //\n    }\n}\n",
        ),
        t(
            "cast",
            "Custom Cast",
            "Laravel",
            "app/Casts/{{name}}.php",
            "<?php\n\nnamespace App\\Casts;\n\nuse Illuminate\\Contracts\\Database\\Eloquent\\CastsAttributes;\n\nclass {{name}} implements CastsAttributes\n{\n    public function get($model, string $key, $value, array $attributes)\n    {\n        return $value;\n    }\n\n    public function set($model, string $key, $value, array $attributes)\n    {\n        return $value;\n    }\n}\n",
        ),
        t(
            "console",
            "Artisan Command",
            "Laravel",
            "app/Console/Commands/{{name}}.php",
            "<?php\n\nnamespace App\\Console\\Commands;\n\nuse Illuminate\\Console\\Command;\n\nclass {{name}} extends Command\n{\n    protected $signature = 'app:command';\n    protected $description = '';\n\n    public function handle(): int\n    {\n        return self::SUCCESS;\n    }\n}\n",
        ),
        t(
            "enum",
            "Enum",
            "PHP",
            "app/Enums/{{name}}.php",
            "<?php\n\nnamespace App\\Enums;\n\nenum {{name}}: string\n{\n    case Example = 'example';\n}\n",
        ),
        t(
            "action",
            "Single Action",
            "Laravel",
            "app/Actions/{{name}}.php",
            "<?php\n\nnamespace App\\Actions;\n\nclass {{name}}\n{\n    public function handle(): void\n    {\n        //\n    }\n}\n",
        ),
        t(
            "dto",
            "Data Transfer Object",
            "PHP",
            "app/Data/{{name}}.php",
            "<?php\n\nnamespace App\\Data;\n\nfinal readonly class {{name}}\n{\n    public function __construct(\n    ) {}\n}\n",
        ),
        t(
            "seeder",
            "Seeder",
            "Laravel",
            "database/seeders/{{name}}.php",
            "<?php\n\nnamespace Database\\Seeders;\n\nuse Illuminate\\Database\\Seeder;\n\nclass {{name}} extends Seeder\n{\n    public function run(): void\n    {\n        //\n    }\n}\n",
        ),
        t(
            "factory",
            "Model Factory",
            "Laravel",
            "database/factories/{{name}}Factory.php",
            "<?php\n\nnamespace Database\\Factories;\n\nuse Illuminate\\Database\\Eloquent\\Factories\\Factory;\n\nclass {{name}}Factory extends Factory\n{\n    public function definition(): array\n    {\n        return [];\n    }\n}\n",
        ),
        t(
            "pivot-migration",
            "Pivot Table Migration",
            "Laravel",
            "database/migrations/{{timestamp}}_create_{{name}}_table.php",
            "<?php\n\nuse Illuminate\\Database\\Migrations\\Migration;\nuse Illuminate\\Database\\Schema\\Blueprint;\nuse Illuminate\\Support\\Facades\\Schema;\n\nreturn new class extends Migration\n{\n    public function up(): void\n    {\n        Schema::create('{{name}}', function (Blueprint $table) {\n            $table->id();\n            $table->foreignId('first_id')->constrained();\n            $table->foreignId('second_id')->constrained();\n            $table->timestamps();\n        });\n    }\n\n    public function down(): void\n    {\n        Schema::dropIfExists('{{name}}');\n    }\n};\n",
        ),
    ]
}

/// User templates from `.photon/templates/*.json`.
pub fn user_templates(project_root: &str) -> Vec<Template> {
    let dir = PathBuf::from(project_root).join(".photon").join("templates");
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if e.path().extension().and_then(|x| x.to_str()) == Some("json") {
                if let Ok(text) = std::fs::read_to_string(e.path()) {
                    if let Ok(mut t) = serde_json::from_str::<Template>(&text) {
                        t.source = "user".into();
                        out.push(t);
                    }
                }
            }
        }
    }
    out
}

/// Replace `{{ key }}` / `{{key}}` placeholders + auto vars.
pub fn substitute(text: &str, vars: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    let mut all = vars.clone();
    all.entry("timestamp".into()).or_insert_with(timestamp);
    all.entry("year".into()).or_insert_with(|| year());
    for (k, v) in &all {
        out = out.replace(&format!("{{{{{}}}}}", k), v);
        out = out.replace(&format!("{{{{ {} }}}}", k), v);
    }
    out
}

/// Create the file from a resolved template. Errors if it already exists.
/// Returns the workspace-relative path written.
pub fn create(
    project_root: &str,
    template: &Template,
    vars: &HashMap<String, String>,
) -> Result<String, String> {
    let rel = substitute(&template.filename, vars);
    let full = PathBuf::from(project_root).join(&rel);
    if full.exists() {
        return Err(format!("{} already exists", rel));
    }
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let body = substitute(&template.body, vars);
    std::fs::write(&full, body).map_err(|e| e.to_string())?;
    Ok(rel.replace('\\', "/"))
}

fn timestamp() -> String {
    // Prefer a real Laravel-style timestamp via `date`; fall back to epoch.
    if let Ok(out) = Command::new("date").arg("+%Y_%m_%d_%H%M%S").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("ts_{secs}")
}

fn year() -> String {
    if let Ok(out) = Command::new("date").arg("+%Y").output() {
        if out.status.success() {
            return String::from_utf8_lossy(&out.stdout).trim().to_string();
        }
    }
    "2026".into()
}
