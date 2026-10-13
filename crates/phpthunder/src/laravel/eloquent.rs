//! Eloquent at query time: how Laravel's magic forwarding changes what a
//! call on a model or a query builder returns. A model forwards unknown
//! static and instance calls to `Builder<Model>` (`User::where(...)`), and
//! a builder forwards unknown calls to the model's scopes
//! (`User::query()->active()`). Also maps framework contracts to the
//! classes the container gives back (`Auth::user()` is the app's user model).

use crate::index::Index;
use crate::symbol::{MemberTag, Ty};

/// Base class of every Eloquent model.
pub const ELOQUENT_MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";

/// Eloquent's query builder; generic over the model (`Builder<TModel>`).
pub const ELOQUENT_BUILDER: &str = "Illuminate\\Database\\Eloquent\\Builder";

/// Classes tried, in order, as the authenticated user model.
const USER_MODELS: &[&str] = &["App\\Models\\User", "App\\User"];

/// Whether `class` extends Eloquent's `Model` (directly or not). Walks the
/// hierarchy; used per member lookup, not per symbol.
pub fn is_model(index: &Index, class: &str) -> bool {
    !class.eq_ignore_ascii_case(ELOQUENT_MODEL)
        && index
            .hierarchy(class)
            .iter()
            .any(|c| c.eq_ignore_ascii_case(ELOQUENT_MODEL))
}

/// Whether `class` is Eloquent's builder or a subclass of it.
pub fn is_builder(index: &Index, class: &str) -> bool {
    index
        .hierarchy(class)
        .iter()
        .any(|c| c.eq_ignore_ascii_case(ELOQUENT_BUILDER))
}

/// Where a method missing on `receiver` is looked up next, following
/// Eloquent's `__call` / `__callStatic` forwarding: a model forwards to
/// `Builder<model>`, a `Builder<model>` to the model (for its scopes).
/// `None` when no forwarding applies. Only methods are forwarded.
pub fn forward(index: &Index, receiver: &Ty, tag: MemberTag) -> Option<Ty> {
    if tag != MemberTag::Method {
        return None;
    }
    let class = receiver.class.as_deref()?;
    if is_model(index, class) {
        return Some(Ty {
            class: Some(ELOQUENT_BUILDER.to_string()),
            elem: Some(class.to_string()),
            args: vec![class.to_string()],
            conditional: None,
        });
    }
    let model = receiver.args.first().filter(|m| !m.is_empty())?;
    (is_builder(index, class) && is_model(index, model)).then(|| Ty::class(model.clone()))
}

/// Return type of a framework method whose PHPDoc says only `mixed`:
/// `Request::user()` is the authenticated user. `None` otherwise.
pub fn untyped_return(index: &Index, class: &str, method: &str) -> Option<Ty> {
    let request = method.eq_ignore_ascii_case("user")
        && index
            .hierarchy(class)
            .iter()
            .any(|c| c.eq_ignore_ascii_case("Illuminate\\Http\\Request"));
    request.then(|| {
        refine(
            index,
            Ty::class("Illuminate\\Contracts\\Auth\\Authenticatable"),
        )
    })
}

/// Replaces framework contracts with the class Laravel actually returns:
/// the auth factory is the `AuthManager` (which mixes in the guards), and an
/// authenticated user is the application's user model when one is indexed.
pub fn refine(index: &Index, ty: Ty) -> Ty {
    let Some(class) = ty.class.as_deref() else {
        return ty;
    };
    let replacement = match class.to_ascii_lowercase().as_str() {
        "illuminate\\contracts\\auth\\factory" => Some("Illuminate\\Auth\\AuthManager"),
        "illuminate\\contracts\\auth\\authenticatable" => USER_MODELS
            .iter()
            .copied()
            .find(|m| !index.classes(m).is_empty()),
        _ => None,
    };
    match replacement {
        Some(r) => Ty::class(r),
        None => ty,
    }
}
