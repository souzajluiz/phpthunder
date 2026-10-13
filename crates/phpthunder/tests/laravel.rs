#![expect(
    clippy::unwrap_used,
    reason = "test helpers panic on purpose: a failed lookup is a failed test"
)]

//! Laravel type inference end to end on a miniature framework: Eloquent
//! forwarding between models and their builder, conditional return types
//! of helpers, migration columns as model attributes, and completion
//! visibility.

use phpthunder::analysis::{complete, definitions_at};
use phpthunder::index::{Index, index_source};
use std::path::PathBuf;

/// Just enough of Laravel, Carbon and an app to exercise the inference.
const FRAMEWORK: &str = r#"<?php
namespace Carbon { class Carbon { public function format(string $f): string {} } }

namespace Illuminate\Contracts\Auth { interface Authenticatable {} }

namespace Illuminate\Database\Concerns {
    /** @template TValue */
    trait BuildsQueries {
        /** @return TValue|null */
        public function first() {}
    }
}

namespace Illuminate\Database\Eloquent {
    /** @template TModel */
    class Collection { public function count(): int {} }

    /**
     * @template TModel of \Illuminate\Database\Eloquent\Model
     */
    class Builder {
        /** @use \Illuminate\Database\Concerns\BuildsQueries<TModel> */
        use \Illuminate\Database\Concerns\BuildsQueries;
        /** @return $this */
        public function where($column, $value = null) {}
        /** @return ($id is (array<mixed>) ? \Illuminate\Database\Eloquent\Collection<int, TModel> : TModel|null) */
        public function find($id) {}
        /** @return TModel */
        public function create(array $attributes = []) {}
    }

    abstract class Model {
        protected $connection;
        public function save(): bool {}
        /** @return \Illuminate\Database\Eloquent\Builder<static> */
        public static function query() {}
        public static function __callStatic($method, $parameters) {}
    }
}

namespace Illuminate\Http {
    class Request {
        public function input($key = null) {}
        /** @return mixed */
        public function user($guard = null) {}
    }
}

namespace Illuminate\Foundation { class Application { public function make($abstract) {} } }

namespace {
    /** @return ($key is null ? \Illuminate\Http\Request : mixed) */
    function request($key = null) {}

    /**
     * @template TClass of object
     * @param string|class-string<TClass>|null $abstract
     * @return ($abstract is class-string<TClass> ? TClass : ($abstract is null ? \Illuminate\Foundation\Application : mixed))
     */
    function app($abstract = null) {}
}

namespace App\Models {
    use Illuminate\Database\Eloquent\Model;

    class User extends Model implements \Illuminate\Contracts\Auth\Authenticatable {
        public function name(): string {}
    }

    class Post extends Model {
        protected $fillable = ['title'];
        public function scopeActive($query) {}
        private function secret() {}
    }
}
"#;

/// A migration declaring the `posts` table.
const MIGRATION: &str = r#"<?php
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class {
    public function up(): void {
        Schema::create('posts', function (Blueprint $table) {
            $table->id();
            $table->string('title');
            $table->foreignId('user_id')->constrained();
            $table->timestamps();
        });
    }
};
"#;

/// Index holding the framework stub and the migration.
fn index() -> Index {
    let mut idx = Index::default();
    idx.update(PathBuf::from("/framework.php"), index_source(FRAMEWORK));
    idx.update(PathBuf::from("/migration.php"), index_source(MIGRATION));
    idx
}

/// Prefix of the code under test: a controller method importing the models.
const HEAD: &str = "<?php\nnamespace App\\Http;\nuse App\\Models\\Post;\nuse Illuminate\\Http\\Request;\nfunction demo(Request $request) {\n";

/// `Owner::member` for the last identifier of `code` (the name the caret
/// sits on is the last `->x(` / `->x` in the snippet).
fn target(code: &str) -> Vec<String> {
    let src = format!("{HEAD}{code}\n}}\n");
    let marker = src.rfind("->").unwrap() + 2;
    definitions_at(&index(), &src, marker)
        .into_iter()
        .map(|d| format!("{}::{}", d.symbol.fqn, d.symbol.name))
        .collect()
}

/// Completion labels at the end of `code`.
fn labels(code: &str) -> Vec<String> {
    let src = format!("{HEAD}{code}");
    let byte = src.len();
    let src = format!("{src}\n}}\n");
    complete(&index(), &src, byte)
        .into_iter()
        .map(|c| c.label)
        .collect()
}

#[test]
fn static_model_calls_should_forward_to_the_builder() {
    assert_eq!(
        target("Post::where('a', 1)->first()->save();"),
        ["Illuminate\\Database\\Eloquent\\Model::save"]
    );
    assert_eq!(
        target("Post::create([])->save();"),
        ["Illuminate\\Database\\Eloquent\\Model::save"]
    );
}

#[test]
fn conditional_return_types_should_follow_the_argument() {
    assert_eq!(
        target("Post::find(1)->save();"),
        ["Illuminate\\Database\\Eloquent\\Model::save"]
    );
    assert_eq!(
        target("Post::find([1, 2])->count();"),
        ["Illuminate\\Database\\Eloquent\\Collection::count"]
    );
    assert_eq!(
        target("request()->input();"),
        ["Illuminate\\Http\\Request::input"]
    );
    assert_eq!(
        target("app(Post::class)->save();"),
        ["Illuminate\\Database\\Eloquent\\Model::save"]
    );
    assert_eq!(
        target("app()->make('x');"),
        ["Illuminate\\Foundation\\Application::make"]
    );
}

#[test]
fn builder_should_reach_model_scopes() {
    assert_eq!(
        target("Post::query()->active()->first()->save();"),
        ["Illuminate\\Database\\Eloquent\\Model::save"]
    );
}

#[test]
fn request_user_should_be_the_user_model() {
    assert_eq!(
        target("$request->user()->name();"),
        ["App\\Models\\User::name"]
    );
}

#[test]
fn migration_columns_should_be_model_attributes() {
    assert_eq!(
        target("(new Post)->created_at->format('Y');"),
        ["Carbon\\Carbon::format"]
    );
    let attrs = labels("(new Post)->");
    for column in ["id", "title", "user_id", "created_at", "updated_at"] {
        assert!(
            attrs.iter().any(|l| l == column),
            "{column} missing: {attrs:?}"
        );
    }
}

#[test]
fn completion_should_match_camel_humps_and_middle_words() {
    // A word in the middle: `At` finds `created_at` and `updated_at`.
    let middle = labels("(new Post)->At");
    assert!(middle.iter().any(|l| l == "created_at"), "{middle:?}");
    assert!(middle.iter().any(|l| l == "updated_at"), "{middle:?}");
    // Humps: `uI` finds `user_id`, not `updated_at`.
    let humps = labels("(new Post)->uI");
    assert_eq!(humps, ["user_id"]);
}

#[test]
fn completion_should_hide_members_out_of_reach() {
    let attrs = labels("(new Post)->");
    assert!(
        !attrs.iter().any(|l| l == "connection" || l == "secret"),
        "{attrs:?}"
    );
    assert!(attrs.iter().any(|l| l == "save"));
}

#[test]
fn static_completion_on_a_model_should_list_builder_methods() {
    let statics = labels("Post::");
    for name in ["query", "where", "find", "create", "active"] {
        assert!(
            statics.iter().any(|l| l == name),
            "{name} missing: {statics:?}"
        );
    }
}
