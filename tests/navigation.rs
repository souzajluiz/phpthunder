#![expect(
    clippy::unwrap_used,
    reason = "test helpers panic on purpose: a failed lookup is a failed test"
)]

//! End-to-end checks against the fixture project in tests/fixtures/app.

use parking_lot::RwLock;
use phpthunder::analysis::{Candidate, complete, definitions_at, find_references};
use phpthunder::index::Index;
use phpthunder::stubs::Kind;
use phpthunder::workspace::{index_files, php_files};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/app")
}

fn indexed() -> Index {
    let index = RwLock::new(Index::default());
    index_files(&index, &php_files(&root()), |_, _| {});
    index.into_inner()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap()
}

/// Byte offset of the `nth` occurrence of `needle`, plus `shift` bytes.
fn at(src: &str, needle: &str, shift: usize) -> usize {
    src.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} not in source"))
        + shift
}

/// Resolves the identifier at `needle` in `file` and returns "file:line name".
fn goto(index: &Index, file: &str, needle: &str, shift: usize) -> Vec<String> {
    let src = read(file);
    definitions_at(index, &src, at(&src, needle, shift))
        .iter()
        .map(|d| {
            let rel = d
                .path
                .strip_prefix(root().join("src"))
                .unwrap()
                .display()
                .to_string();
            format!(
                "{rel}:{} {}",
                d.symbol.name_span.start.line + 1,
                d.symbol.name
            )
        })
        .collect()
}

const CONTROLLER: &str = "src/Http/UserController.php";

#[test]
fn goto_class_through_import_and_alias() {
    let idx = indexed();
    assert_eq!(
        goto(&idx, CONTROLLER, "User $user", 0),
        ["Models/User.php:8 User"]
    );
    assert_eq!(
        goto(&idx, CONTROLLER, "S::upper", 0),
        ["Support/helpers.php:7 Str"]
    );
    assert_eq!(
        goto(&idx, CONTROLLER, "use App\\Models\\Status", 19),
        ["Models/Status.php:5 Status"]
    );
}

#[test]
fn goto_member_uses_inferred_types() {
    let idx = indexed();
    // Parameter type hint.
    assert_eq!(
        goto(&idx, CONTROLLER, "getName()) .", 0),
        ["Models/User.php:18 getName"]
    );
    // Method inherited from a trait.
    assert_eq!(
        goto(&idx, CONTROLLER, "greet()", 0),
        ["Support/Greets.php:7 greet"]
    );
    // `$fresh = User::create()` where create() returns `static` in the parent.
    assert_eq!(
        goto(&idx, CONTROLLER, "save()", 0),
        ["Models/Model.php:14 save"]
    );
    // Chained call through a declared return type.
    assert_eq!(
        goto(&idx, CONTROLLER, "format()", 0),
        ["Models/Address.php:9 format"]
    );
    // Function imported with `use function`, then a method on its return type.
    assert_eq!(
        goto(&idx, CONTROLLER, "current_user()", 0),
        ["Support/helpers.php:16 current_user"]
    );
    assert_eq!(
        goto(&idx, CONTROLLER, "getName();", 0),
        ["Models/User.php:18 getName"]
    );
    // Constants and enum cases.
    assert_eq!(
        goto(&idx, CONTROLLER, "ROLE", 0),
        ["Models/User.php:12 ROLE"]
    );
    assert_eq!(
        goto(&idx, CONTROLLER, "Active", 0),
        ["Models/Status.php:7 Active"]
    );
    // Promoted constructor property via $this.
    assert_eq!(
        goto(&idx, "src/Models/User.php", "address;", 0),
        ["Models/User.php:14 address"]
    );
    // Static method declared on the parent class.
    assert_eq!(
        goto(&idx, CONTROLLER, "create()", 0),
        ["Models/Model.php:9 create"]
    );
}

#[test]
fn find_usages_across_files() {
    let idx = indexed();
    let src = read("src/Models/User.php");
    let defs = definitions_at(&idx, &src, at(&src, "function getName", 9));
    assert_eq!(defs.len(), 1);
    let usages = find_references(&idx, &defs, |p| std::fs::read_to_string(p).ok());
    let found: Vec<String> = usages
        .iter()
        .map(|u| {
            let rel = u
                .path
                .strip_prefix(root().join("src"))
                .unwrap()
                .display()
                .to_string();
            format!(
                "{rel}:{}{}",
                u.span.start.line + 1,
                if u.is_declaration { " decl" } else { "" }
            )
        })
        .collect();
    // The interface method `HasName::getName` is a different declaration and is not included.
    assert_eq!(
        found,
        [
            "Http/PostController.php:14",
            "Http/UserController.php:19",
            "Http/UserController.php:20",
            "Models/User.php:18 decl"
        ]
    );
}

#[test]
fn find_usages_of_class() {
    let idx = indexed();
    let src = read("src/Models/Address.php");
    let defs = definitions_at(&idx, &src, at(&src, "class Address", 6));
    let usages = find_references(&idx, &defs, |p| std::fs::read_to_string(p).ok());
    let lines: Vec<String> = usages
        .iter()
        .map(|u| {
            format!(
                "{}:{}",
                u.path.file_name().unwrap().to_str().unwrap(),
                u.span.start.line + 1
            )
        })
        .collect();
    assert_eq!(lines, ["Address.php:5", "User.php:14", "User.php:23"]);
}

fn labels(c: &[Candidate]) -> Vec<&str> {
    let mut v: Vec<&str> = c.iter().map(|c| c.label.as_str()).collect();
    v.sort();
    v
}

#[test]
fn completes_instance_members_with_inheritance() {
    let idx = indexed();
    let src = read(CONTROLLER);
    let byte = at(&src, "$user->greet", 7);
    let all = complete(&idx, &src, byte);
    assert_eq!(
        labels(&all),
        [
            "__construct",
            "address",
            "address",
            "create",
            "getName",
            "greet",
            "name",
            "save"
        ]
    );
    let method = all.iter().find(|c| c.label == "getName").unwrap();
    assert_eq!(method.insert_text, "getName(");
    assert_eq!(method.kind, Kind::Method);
}

#[test]
fn completes_static_members_and_enum_cases() {
    let idx = indexed();
    let src = read(CONTROLLER);
    let statics = complete(&idx, &src, at(&src, "User::create", 6));
    // `User::cr(...)` is a call, so `::class` is not offered there.
    assert_eq!(labels(&statics), ["$table", "ROLE", "create"]);
    let cases = complete(&idx, &src, at(&src, "Status::Active", 8));
    assert_eq!(labels(&cases), ["Active", "Banned", "class"]);
}

#[test]
fn completes_classes_with_auto_import() {
    let idx = indexed();
    let src = read(CONTROLLER).replace("$status = Status::Active;", "$a = new Addr");
    let byte = at(&src, "new Addr", 8);
    let got = complete(&idx, &src, byte);
    let address = got
        .iter()
        .find(|c| c.label == "Address")
        .expect("Address offered");
    let (offset, text) = address.import.clone().expect("import edit");
    assert_eq!(text, "\nuse App\\Models\\Address;");
    assert!(src[..offset].ends_with("use function App\\Support\\current_user;"));
    // Already imported class: no edit.
    let src2 = read(CONTROLLER).replace("$status = Status::Active;", "$x = new Us");
    let user = complete(&idx, &src2, at(&src2, "new Us", 6));
    let user = user.iter().find(|c| c.label == "User").unwrap();
    assert!(user.import.is_none());
}

#[test]
fn completes_local_variables() {
    let idx = indexed();
    let src = read(CONTROLLER).replace("$status = Status::Active;", "$");
    let all = complete(&idx, &src, at(&src, "$\n", 1));
    assert_eq!(labels(&all), ["$city", "$fresh", "$this", "$user"]);
    let src = read(CONTROLLER).replace("$status = Status::Active;", "$f");
    let got = complete(&idx, &src, at(&src, "$f\n", 2));
    assert_eq!(labels(&got), ["$fresh"]);
}

const POSTS: &str = "src/Http/PostController.php";

#[test]
fn phpdoc_types_drive_resolution() {
    let idx = indexed();
    // `@param Post $post` + `@property-read User $author`.
    assert_eq!(
        goto(&idx, POSTS, "author->", 0),
        ["Models/Post.php:10 author"]
    );
    assert_eq!(
        goto(&idx, POSTS, "getName()", 0),
        ["Models/User.php:18 getName"]
    );
    // `foreach` over `@return Comment[]`, then `@return User` on writer().
    assert_eq!(
        goto(&idx, POSTS, "writer()", 0),
        ["Models/Comment.php:15 writer"]
    );
    assert_eq!(
        goto(&idx, POSTS, "greet()", 0),
        ["Support/Greets.php:7 greet"]
    );
    // `$post->comments[0]` with `@var Comment[]`.
    assert_eq!(
        goto(&idx, POSTS, "body()", 0),
        ["Models/Comment.php:7 body"]
    );
    // Inline `/** @var Post $draft */`.
    assert_eq!(
        goto(&idx, POSTS, "approved();", 0),
        ["Models/Post.php:23 approved"]
    );
    // `@method static Builder published()` then `@return Post|null`.
    assert_eq!(
        goto(&idx, POSTS, "published()", 0),
        ["Models/Post.php:11 published"]
    );
    assert_eq!(
        goto(&idx, POSTS, "first()", 0),
        ["Support/Builder.php:10 first"]
    );
    assert_eq!(
        goto(&idx, POSTS, "comments;", 0),
        ["Models/Post.php:16 comments"]
    );
}

#[test]
fn completes_phpdoc_virtual_members() {
    let idx = indexed();
    let src = read(POSTS);
    let got = complete(&idx, &src, at(&src, "$post->author", 7));
    assert_eq!(
        labels(&got),
        [
            "approved",
            "author",
            "comments",
            "create",
            "published",
            "save"
        ]
    );
    let statics = complete(&idx, &src, at(&src, "Post::published", 6));
    assert_eq!(labels(&statics), ["$table", "create", "published"]);
}
