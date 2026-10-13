//! Recognising a string argument that names a route, view, config key,
//! translation key or environment variable.
//! Works on plain text, so it serves PHP and Blade files alike.

use super::*;

/// Calls whose first argument names a key, by the text right before `(`.
const CALLS: &[(&str, KeyKind)] = &[
    ("route", KeyKind::Route),
    ("to_route", KeyKind::Route),
    ("routeIs", KeyKind::Route),
    ("URL::route", KeyKind::Route),
    ("URL::signedRoute", KeyKind::Route),
    ("URL::temporarySignedRoute", KeyKind::Route),
    ("Route::has", KeyKind::Route),
    ("Route::is", KeyKind::Route),
    ("Route::currentRouteNamed", KeyKind::Route),
    ("view", KeyKind::View),
    ("View::make", KeyKind::View),
    ("View::exists", KeyKind::View),
    ("View::first", KeyKind::View),
    ("markdown", KeyKind::View),
    ("@include", KeyKind::View),
    ("@includeIf", KeyKind::View),
    ("@includeFirst", KeyKind::View),
    ("@extends", KeyKind::View),
    ("@each", KeyKind::View),
    ("@component", KeyKind::View),
    ("config", KeyKind::Config),
    ("Config::get", KeyKind::Config),
    ("Config::set", KeyKind::Config),
    ("Config::has", KeyKind::Config),
    ("Config::string", KeyKind::Config),
    ("Config::integer", KeyKind::Config),
    ("Config::float", KeyKind::Config),
    ("Config::boolean", KeyKind::Config),
    ("Config::array", KeyKind::Config),
    ("__", KeyKind::Translation),
    ("trans", KeyKind::Translation),
    ("trans_choice", KeyKind::Translation),
    ("Lang::get", KeyKind::Translation),
    ("Lang::has", KeyKind::Translation),
    ("Lang::choice", KeyKind::Translation),
    ("@lang", KeyKind::Translation),
    ("@choice", KeyKind::Translation),
    ("env", KeyKind::Env),
    ("middleware", KeyKind::Middleware),
    ("withoutMiddleware", KeyKind::Middleware),
    ("can", KeyKind::Ability),
    ("cannot", KeyKind::Ability),
    ("cant", KeyKind::Ability),
    ("authorize", KeyKind::Ability),
    ("Gate::allows", KeyKind::Ability),
    ("Gate::denies", KeyKind::Ability),
    ("Gate::check", KeyKind::Ability),
    ("Gate::any", KeyKind::Ability),
    ("Gate::none", KeyKind::Ability),
    ("Gate::authorize", KeyKind::Ability),
    ("Gate::inspect", KeyKind::Ability),
    ("Gate::has", KeyKind::Ability),
    ("@can", KeyKind::Ability),
    ("@cannot", KeyKind::Ability),
    ("@canany", KeyKind::Ability),
    ("@elsecan", KeyKind::Ability),
    ("@elsecannot", KeyKind::Ability),
    ("Auth::guard", KeyKind::Guard),
    ("Auth::shouldUse", KeyKind::Guard),
    ("auth", KeyKind::Guard),
    ("guard", KeyKind::Guard),
    ("@auth", KeyKind::Guard),
    ("@guest", KeyKind::Guard),
    ("Storage::disk", KeyKind::Disk),
    ("Storage::fake", KeyKind::Disk),
    ("disk", KeyKind::Disk),
    ("Cache::store", KeyKind::CacheStore),
    ("cache()->store", KeyKind::CacheStore),
    ("Queue::connection", KeyKind::QueueConnection),
    ("onConnection", KeyKind::QueueConnection),
    ("Log::channel", KeyKind::LogChannel),
    ("Log::stack", KeyKind::LogChannel),
    ("DB::connection", KeyKind::DbConnection),
    ("Schema::connection", KeyKind::DbConnection),
    ("Mail::mailer", KeyKind::Mailer),
    ("Feature::active", KeyKind::Feature),
    ("Feature::inactive", KeyKind::Feature),
    ("Feature::value", KeyKind::Feature),
    ("Feature::activate", KeyKind::Feature),
    ("Feature::deactivate", KeyKind::Feature),
    ("Feature::when", KeyKind::Feature),
    ("Feature::unless", KeyKind::Feature),
    ("Feature::allAreActive", KeyKind::Feature),
    ("Feature::someAreActive", KeyKind::Feature),
    ("@feature", KeyKind::Feature),
    ("Inertia::render", KeyKind::InertiaPage),
    ("inertia", KeyKind::InertiaPage),
    ("@livewire", KeyKind::Livewire),
    ("Livewire::test", KeyKind::Livewire),
    ("@push", KeyKind::Stack),
    ("@prepend", KeyKind::Stack),
    ("@pushOnce", KeyKind::Stack),
    ("@prependOnce", KeyKind::Stack),
    ("@stack", KeyKind::Stack),
    ("@section", KeyKind::Section),
    ("@yield", KeyKind::Section),
    ("@hasSection", KeyKind::Section),
    ("@sectionMissing", KeyKind::Section),
    ("@vite", KeyKind::ViteAsset),
    ("Vite::asset", KeyKind::ViteAsset),
    ("Vite::content", KeyKind::ViteAsset),
    ("asset", KeyKind::Asset),
    ("secure_asset", KeyKind::Asset),
    ("Artisan::call", KeyKind::Command),
    ("Artisan::queue", KeyKind::Command),
    ("callSilent", KeyKind::Command),
    ("callSilently", KeyKind::Command),
    ("command", KeyKind::Command),
    ("app", KeyKind::Binding),
    ("resolve", KeyKind::Binding),
    ("App::make", KeyKind::Binding),
    ("App::bound", KeyKind::Binding),
    ("app()->make", KeyKind::Binding),
];

/// Kinds whose calls also take a list of keys (`->middleware(['auth', …])`).
const LIST_KINDS: &[KeyKind] = &[
    KeyKind::Middleware,
    KeyKind::Ability,
    KeyKind::View,
    KeyKind::LogChannel,
    KeyKind::Feature,
    KeyKind::ViteAsset,
];

/// Array keys whose value names keys (`Route::group(['middleware' => …])`).
const ARRAY_KEYS: &[(&str, KeyKind)] = &[("middleware", KeyKind::Middleware)];

/// The key reference whose string contains `offset`: the first argument
/// of a known call (`route('…')`, `->view('…')`, `@include('…')`), an
/// element of a list given to one (`->middleware(['auth', '…'])`), the
/// value of a known array key (`'middleware' => '…'`), or a rule inside a
/// validation rule string.
pub fn key_at(src: &str, offset: usize) -> Option<KeyRef> {
    let line_start = src.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[offset..].find('\n').map_or(src.len(), |i| offset + i);
    let line = &src[line_start..line_end];
    let caret = offset - line_start;
    // Find the quoted string around the caret on this line.
    let mut open: Option<(usize, char)> = None;
    let mut found = None;
    for (i, c) in line.char_indices() {
        match open {
            None if c == '\'' || c == '"' => open = Some((i, c)),
            Some((start, quote)) if c == quote && !line[..i].ends_with('\\') => {
                if (start + 1..=i).contains(&caret) {
                    found = Some((start, i));
                    break;
                }
                open = None;
            }
            _ => {}
        }
    }
    let (quote_at, close_at) = match (found, open) {
        (Some(f), _) => f,
        // Unterminated string being typed: runs to the end of the line.
        (None, Some((start, _))) if caret > start => (start, line.len()),
        _ => return None,
    };
    let start = line_start + quote_at + 1;
    let end = line_start + close_at;
    let Some(kind) = context_kind(&line[..quote_at]) else {
        return rules::rule_at(src, start, end, offset);
    };
    let key = KeyRef {
        kind,
        start,
        end,
        prefix: src[start..offset].to_string(),
        value: src[start..end].to_string(),
    };
    Some(ability_in_middleware(key))
}

/// What the string after `before` names, from the call or array key that
/// takes it. List elements skip back over earlier strings to the `[`.
fn context_kind(before: &str) -> Option<KeyKind> {
    let before_all = before.trim_end();
    let mut before = before_all;
    let mut in_list = false;
    loop {
        if let Some(rest) = before.strip_suffix('[') {
            before = rest.trim_end();
            in_list = true;
            break;
        }
        let Some(rest) = before.strip_suffix(',') else {
            // Skipped earlier strings without reaching `[`: a later
            // argument (`route('a', '…')`), not a list element.
            if before.len() < before_all.len() && !in_list {
                return None;
            }
            break;
        };
        // An earlier element must be a plain string (`'auth', `).
        let rest = rest.trim_end();
        let quote = rest
            .chars()
            .next_back()
            .filter(|c| *c == '\'' || *c == '"')?;
        let open = rest[..rest.len() - 1].rfind(quote)?;
        before = rest[..open].trim_end();
    }
    if let Some(rest) = before.strip_suffix("=>") {
        let rest = rest.trim_end();
        return ARRAY_KEYS.iter().find_map(|(key, kind)| {
            let quoted = [format!("'{key}'"), format!("\"{key}\"")];
            quoted
                .iter()
                .any(|q| rest.ends_with(q.as_str()))
                .then_some(*kind)
        });
    }
    let before = before.strip_suffix('(')?.trim_end();
    let kind = CALLS.iter().find_map(|(call, kind)| {
        let rest = before.strip_suffix(call)?;
        let boundary = rest
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'));
        let method = rest.ends_with("->") || rest.ends_with("?->");
        (boundary || method).then_some(*kind)
    })?;
    (!in_list || LIST_KINDS.contains(&kind)).then_some(kind)
}

/// `can:update` in a middleware string names an ability after `can:`.
fn ability_in_middleware(key: KeyRef) -> KeyRef {
    match key.value.strip_prefix("can:") {
        Some(rest) if key.kind == KeyKind::Middleware && key.prefix.starts_with("can:") => {
            let ability = rest.split(',').next().unwrap_or_default();
            KeyRef {
                kind: KeyKind::Ability,
                start: key.start + 4,
                end: key.start + 4 + ability.len(),
                prefix: key.prefix[4..].to_string(),
                value: ability.to_string(),
            }
        }
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> Option<KeyRef> {
        let offset = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        key_at(&src, offset)
    }

    #[test]
    fn key_at_should_recognise_calls_methods_and_directives() {
        let r = at("return redirect()->route('users.sh|ow');").unwrap();
        assert_eq!(
            (r.kind, r.prefix.as_str(), r.value.as_str()),
            (KeyKind::Route, "users.sh", "users.show")
        );
        assert_eq!(at("view('home|')").unwrap().kind, KeyKind::View);
        assert_eq!(at("@include('partials.n|av')").unwrap().kind, KeyKind::View);
        assert_eq!(
            at("$x = config(\"app.na|\");").unwrap().kind,
            KeyKind::Config
        );
        assert_eq!(at("Config::get('app|').").unwrap().kind, KeyKind::Config);
        assert_eq!(at("__('auth.fa|')").unwrap().kind, KeyKind::Translation);
        assert_eq!(at("@lang('auth.fa|')").unwrap().kind, KeyKind::Translation);
        assert_eq!(at("env('APP_|')").unwrap().kind, KeyKind::Env);
    }

    #[test]
    fn key_at_should_accept_an_unterminated_string() {
        let r = at("route('adm|").unwrap();
        assert_eq!(r.prefix, "adm");
    }

    #[test]
    fn key_at_should_ignore_other_strings() {
        assert_eq!(at("echo 'route(|x)';"), None);
        assert_eq!(at("myroute('a|')"), None);
        assert_eq!(at("route($name, 'a|')"), None);
        assert_eq!(at("route('a', 'b|')"), None);
    }

    #[test]
    fn key_at_should_read_list_elements_of_list_calls() {
        let r = at("->middleware(['auth', 'verif|ied'])").unwrap();
        assert_eq!((r.kind, r.prefix.as_str()), (KeyKind::Middleware, "verif"));
    }

    #[test]
    fn key_at_should_not_read_list_elements_of_other_calls() {
        assert_eq!(at("route(['a', 'b|'])"), None);
    }

    #[test]
    fn key_at_should_read_middleware_array_values() {
        assert_eq!(
            at("Route::group(['middleware' => 'au|th'], fn () => 1);").map(|r| r.kind),
            Some(KeyKind::Middleware)
        );
    }

    #[test]
    fn key_at_should_read_abilities_after_can_middleware() {
        let r = at("->middleware('can:upd|ate,post')").unwrap();
        assert_eq!((r.kind, r.value.as_str()), (KeyKind::Ability, "update"));
    }

    #[test]
    fn key_at_should_read_new_kinds() {
        assert_eq!(
            at("Storage::disk('s|')").map(|r| r.kind),
            Some(KeyKind::Disk)
        );
        assert_eq!(at("@push('scr|')").map(|r| r.kind), Some(KeyKind::Stack));
        assert_eq!(
            at("Inertia::render('Us|')").map(|r| r.kind),
            Some(KeyKind::InertiaPage)
        );
    }
}
