# phpthunder

A fast PHP language server written in Rust.

## Features

- **Go to definition** for classes, interfaces, traits, enums, functions, methods,
  properties, class constants, and enum cases
- **Find usages** across the project
- **Completion** for members after `->` and `::`, class and function names (with
  automatic `use` import), local variables and keywords; camel-hump and
  middle-of-word matching (`gUN` and `Name` find `getUserName`), ranked by
  match quality, how often you pick each proposal, and proximity
- **Hover** with the declaration signature and fully qualified name
- **Inspections and quick fixes**: undefined classes, functions, methods,
  properties and variables, unused imports; import class, add method or
  property, remove import
- **Refactorings**: rename (symbol and class file), optimize imports, code
  generation (constructor, accessors, implement and override methods)
- **Document and workspace symbols**
- **Laravel**: Eloquent forwarding and conventions, migration columns as
  attributes, route/view/config/translation/env keys

Name resolution follows PHP rules: namespaces, `use` imports and aliases, group
imports, `use function`, global function fallback, `self`/`static`/`parent`.
Member lookup walks the inheritance chain (parent classes, interfaces, traits).
Variable types are inferred from `new`, parameter type hints, `$this`, promoted
constructor properties, and declared return types (including `static`), so
chains like `User::create()->address()->format()` resolve.

PHPDoc is understood where native types are missing or too vague:

- `@return`, `@var` (on properties and inline `/** @var Foo $x */`), `@param`
- `@property`, `@property-read`, `@property-write`, `@method [static]` as
  virtual members (go to definition jumps to the docblock line), `@mixin`
- unions and nullables (`Foo|null`, `?Foo`), arrays and generic collections
  (`Foo[]`, `array<int, Foo>`, `Collection<int, Foo>`): the element type flows
  into `foreach` variables and `$items[0]`
- the docblock summary is shown on hover
- generics: `@template` on classes and methods, `@extends` / `@implements` /
  `@use` arguments, and `class-string<T>` parameters, so
  `Collection<int, User>->first()` and `app(User::class)` resolve to `User`

## How it works

1. **Parse** each file with tree-sitter-php.
2. **Stubs**: extract a compact summary of every declaration in the file (name,
   FQN, supertypes, return/property types).
3. **Index**: a declaration index (key -> declarations) and a word index
   (identifier -> files). Files are indexed in parallel and keyed by content
   hash, so unchanged files are never reparsed.
4. **Resolve**: classify the identifier under the caret by syntax position,
   infer the receiver type when needed, and look it up in the index.
5. **Find usages**: the word index narrows the search to files that contain the
   name; each occurrence is resolved and kept only if it points at the same
   declaration.
6. **Completion**: a placeholder identifier is inserted at the caret so
   incomplete code parses; its position decides what to offer.

Indexing runs in the background at startup with progress reporting. Definition,
hover, and completion work on the partial index right away; find usages waits
until indexing completes so results are never missing files.

## IDE

`ide/` is a native desktop IDE written in Rust with [iced](https://iced.rs),
using phpthunder as a library in the same process (no server, no JSON-RPC).

```sh
cargo run --release -p phpthunder-ide -- /path/to/php/project
```

- Project tree, tabs, PHP syntax highlighting (tree-sitter), save
- Completion popup (auto on `->`, `::`, `$` and new words; `⌃Space` to force),
  with automatic `use` imports
- Hover with signature, docblock summary and location
- Go to declaration (`F12` / `⌘B`) and find usages (`⇧F12`) in a bottom panel
- Background indexing with timing in the status bar
- Integrated terminal (`⌥F12`): your shell in a real PTY with VT100 colors,
  scrollback and bracketed paste
- Tests (`⌃⇧R`): Pest or PHPUnit from `vendor/bin`, for the test at the
  caret, the file or the suite; results with failures in the Tests panel
- Reformat (`⌥⌘L`) with the project's Pint, PHP-CS-Fixer or PHPCBF
- PHPStan on save, findings in Problems; Composer scripts (`⌃⌥C`) run in
  the terminal
- Generate (`⌘N`) in a class: constructor, getters, setters, implement
  abstract/interface methods, override parent methods
- Refactorings (also in the `⌥↵` menu): extract variable (`⌥⌘V`) and
  method (`⌥⌘M`) from the selection, inline variable (`⌥⌘N`), change
  signature (`⌘F6`, calls follow), move class to another namespace (`F6`,
  file moves to its PSR-4 folder, imports follow)
- Blade: completion of `<x-` components and their attributes, `@`
  directives and the variables passed to the view
- Live templates: type an abbreviation and press Tab (`fore`, `forek`,
  `pubf`, `__c`, `try`, `it`, `dd`, …) or pick one with `⌘J`
- Laravel: completion and go to declaration for route names, views and
  config keys in `route('…')`, `view('…')`, `config('…')` and Blade
  `@include`/`@extends`; Eloquent relations, scopes, accessors, `$fillable`
  and `$casts` attributes are known members of the model; Blade templates
  get their own highlighting
- Git (the `git` command line): Commit panel (`⌘K`) with changed files,
  staging checkboxes, diff against `HEAD`, commit, commit and push, discard
  (asks twice); push (`⇧⌘K`) and fast-forward pull (`⌘T`); branch picker
  from the title bar to switch or create branches; added/modified/deleted
  marks in the gutter while typing; changed files colored in the tree
- Debugger (Xdebug, DBGp) built around Pest: `⌃⇧D` debugs the test at the
  caret (or its file) with Pest, or PHPUnit when Pest is absent. Click a
  line number (or `⌘F8`) for a breakpoint; `F9` resume, `F8` step over,
  `F7` step into, `⇧F8` step out, `⌘F2` stop. The Debug panel shows the
  call stack, expandable variables, the test output and an evaluate field.
  Needs the Xdebug extension: loaded by PHP, Herd Pro's bundled build, or
  `PHPTHUNDER_XDEBUG=/path/to/xdebug.so`

Project files: the index is cached on disk (`~/Library/Caches/phpthunder`),
so reopening a project only reparses files that changed; changes made outside
the editor (Git, Composer, generators) reload unmodified tabs and refresh the
tree, index and Git status; right-click a tree entry (or the `+` in its
header) for new file, folder or PHP class (namespaced from Composer's PSR-4
map), rename or move, move to Trash, reveal in Finder and copy path. The last
project, its open tabs and carets are restored on start.

Shortcuts: `⌘O` open folder, `⌘S` save, `⌘W` close tab, `⌘1` toggle project
tree, `Esc` close popups/panel.

### Languages

The IDE speaks English and Brazilian Portuguese, following the system
language. Override it with `PHPTHUNDER_LANG=pt-BR` (or `en-US`). Strings
live in [Fluent](https://projectfluent.org) catalogs under
`crates/phpthunder-ide/locales/<language>/`; to add a language, copy
`en-US/`, translate it and register it in `src/i18n/locale.rs`. A test
checks that every language has every message with the same arguments.

## Project layout

```
crates/
  phpthunder/       engine library: syntax, symbol, index, analysis,
                    inspection, refactor (one folder per concept)
    stubs/          PHP built-in declarations, generated by reflection
    tests/          integration tests and the sample PHP project
  phpthunder-lsp/   language server binary (`phpthunder`, LSP over stdio)
  phpthunder-ide/   native IDE (iced): app, editor, palette, search, ui
editors/vscode/     minimal VS Code client for the language server
```

Each concept is a `concept.rs` facade next to a `concept/` folder: private
`mod` declarations plus `pub use file::*;`, so `phpthunder::analysis::complete`
is the only import path and files can move without touching callers.

## Build

```sh
cargo build --release
# binary: target/release/phpthunder (speaks LSP over stdio)
```


Packages for the current platform (macOS `.app` and `.dmg`, Linux `.tar.gz`,
Windows `.zip`), each with the IDE and the `phpthunder` language server:

```sh
scripts/package.sh            # results in dist/
CODESIGN_IDENTITY="Developer ID Application: …" scripts/package.sh   # signed .app
```
## Editor setup

**VS Code**: a minimal client extension lives in `editors/vscode`.

```sh
cargo build --release
cd editors/vscode && npm install && cd ../..
code --extensionDevelopmentPath="$PWD/editors/vscode" crates/phpthunder/tests/fixtures/app
```

The extension uses `target/release/phpthunder` from this repository unless
`phpthunder.path` is set. Disable other PHP language servers (for example
Intelephense) in the test window to avoid duplicate results.

**Neovim** (0.11+):

```lua
vim.lsp.config('phpthunder', {
  cmd = { '/path/to/phpthunder' },
  filetypes = { 'php' },
  root_markers = { 'composer.json', '.git' },
})
vim.lsp.enable('phpthunder')
```

**Helix** (`languages.toml`):

```toml
[language-server.phpthunder]
command = "/path/to/phpthunder"

[[language]]
name = "php"
language-servers = ["phpthunder"]
```

## Tests

```sh
cargo test --workspace
```

`tests/navigation.rs` runs definition, usages, and completion checks against
the sample project in `crates/phpthunder/tests/fixtures/app`.

## Limitations

- The language server offers definitions, usages, completion (ranked, with
  keywords and Laravel keys), hover, symbols, diagnostics, quick fixes,
  organize imports, rename and signature help; code generation, the
  debugger, Git and test tools are IDE-only.
- The debugger needs the Xdebug extension, which phpthunder does not ship.
