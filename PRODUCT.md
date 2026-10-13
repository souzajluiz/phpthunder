# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

Native desktop application (Rust + iced), not web. macOS, Windows and Linux are equally first-class targets; packages ship as macOS `.app`/`.dmg`, Linux `.tar.gz`, Windows `.zip`. None of the web/ios/android design languages apply; follow each desktop OS's conventions where they matter (shortcuts, file dialogs, Finder/Explorer wording).

## Users

PHP developers in general, on any of the three desktop OSes: Laravel, Symfony, WordPress and plain-PHP projects alike. They use the IDE as an all-day daily driver: reading, navigating and refactoring large codebases, running tests, debugging, committing.

## Product Purpose

phpthunder is a PHP IDE (and a standalone LSP server built from the same engine) that gives the deep code intelligence expected from a heavy IDE (resolution, find usages, inspections and quick fixes, refactorings, debugger, tests, Git) while staying a light native app. Success: developers switch to it as their daily editor and never wait on it.

## Positioning

Full IDE depth, native-light footprint. The engine runs in-process (no server, no JSON-RPC), the GUI is 100% Rust with no JVM, Electron, Node or web frontend, so startup is instant and memory stays low. Lightness is a feature, not a trade-off against capability.

## Operating Context

- Long sessions in a dense workbench: project tree, tabs, editor, bottom panels (Find usages, Problems, Tests, Terminal, Debug, Commit).
- Keyboard-first: most actions have shortcuts and an `⌥↵` intention menu; popups (completion, hover, palette) must never block typing.
- Toolchain it drives: Composer, Pest/PHPUnit, PHPStan, Pint/PHP-CS-Fixer/PHPCBF, Xdebug (DBGp), the `git` CLI, the user's shell in a real PTY.
- Laravel awareness: Eloquent, Blade, routes, views, config, translations, env keys.
- Opened projects are real-world PHP repos of any size; the index is cached on disk and reopening must feel immediate.

## Capabilities and Constraints

- GUI must stay pure Rust (iced); no JS/web frontend, no Node. Lightness first: dependencies are chosen for footprint.
- Localized in English (en-US) and Brazilian Portuguese (pt-BR) via Fluent catalogs; every user-facing string goes through them and both languages must stay complete.
- Icons: in-house inline SVG set first; external icon sources only when an icon is missing.
- Typeface assets bundled: Geist and Geist Mono (OFL).
- Also ships `phpthunder` LSP for VS Code, Neovim, Helix.
- License: MIT. Version 0.1.0.
- Undecided: pricing/distribution model beyond the open repo, update mechanism, telemetry (none today).

## Brand Commitments

- Name: phpthunder (lowercase). Existing logo, mark and app icon in `crates/phpthunder-ide/assets/brand/` (`logo.svg`, `logo-light.svg`, `logo-mark.svg`, `app-icon*.svg/png/icns/ico`).
- Never reference or name other IDEs as its origin or inspiration anywhere in the product, docs or copy.

## Evidence on Hand

- Feature list and shortcuts in `README.md`.
- No benchmarks, user counts, testimonials, press or customer logos exist. Do not fabricate speed numbers or adoption claims; any performance claim needs a real measurement first.

## Product Principles

1. Never make the developer wait: latency and memory are product features, measured not assumed.
2. Depth without weight: add capability only in ways that keep the app native and light.
3. Keyboard first, mouse friendly: every frequent action reachable without leaving the keys.
4. Correctness over guesses: resolution follows real PHP rules; never show a result that may be wrong when it can be withheld.
5. Every OS is home: no platform treated as a port.

## Accessibility & Inclusion

Two languages today with full parity required. No formal accessibility standard established yet; open decision.
