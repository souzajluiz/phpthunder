# phpthunder

A fast PHP language server written in Rust.

## Features

- **Go to definition** for classes, interfaces, traits, enums, functions, methods,
  properties, class constants, and enum cases
- **Find usages** across the project
- **Completion** for members after `->` and `::`, class and function names (with
  automatic `use` import), and local variables
- **Hover** with the declaration signature and fully qualified name
- **Document and workspace symbols**

Name resolution follows PHP rules: namespaces, `use` imports and aliases, group
imports, `use function`, global function fallback, `self`/`static`/`parent`.
Member lookup walks the inheritance chain (parent classes, interfaces, traits).
Variable types are inferred from `new`, parameter type hints, `$this`, promoted
constructor properties, and declared return types (including `static`), so
chains like `User::create()->address()->format()` resolve.

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

## Build

```sh
cargo build --release
# binary: target/release/phpthunder (speaks LSP over stdio)
```

## Editor setup

**VS Code**: a minimal client extension lives in `editors/vscode`.

```sh
cargo build --release
cd editors/vscode && npm install && cd ../..
code --extensionDevelopmentPath="$PWD/editors/vscode" tests/fixtures/app
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
cargo test
```

`tests/navigation.rs` runs definition, usages, and completion checks against
the sample project in `tests/fixtures/app`.

## Limitations

- No PHPDoc type inference (`@var`, `@return`, generics) yet.
- No diagnostics, rename, or formatting yet.
- The index lives in memory and is rebuilt on each start.
