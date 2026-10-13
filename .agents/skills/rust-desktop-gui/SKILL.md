---
name: rust-desktop-gui
description: Native Rust desktop GUI guidance. Use when building, changing, testing, debugging, or packaging Windows, macOS, or Linux desktop applications with Iced, egui/eframe, Slint, or Tauri, including event loops, UI-thread work, widgets, themes, accessibility, background work, and native distribution.
---

# Rust Desktop GUI

Use this skill for desktop UI behavior and its framework boundary. It covers
native Rust desktop applications; Tauri is included as a native host with a
system WebView, not as a browser-only application. Keep application policy and
domain state independent of widgets, platform handles, and event-loop types.

Use the [framework-selection reference](references/framework-selection.md) to
choose or confirm a framework. After selecting Iced, load the
[Iced 0.14 reference](references/iced-0.14.md) only if the resolved dependency
is `iced` 0.14.x; verify every framework API against the target's pinned
version before implementation.

## Workflow

1. Inspect the target's `Cargo.toml`, lockfile, Rust toolchain, framework
   version/features, existing UI entry point, platform-specific code, asset
   pipeline, test commands, and packaging/release scripts. Preserve the chosen
   framework and supported targets unless the task explicitly changes them.
2. Define observable interaction flows, error states, keyboard behavior, and
   supported OS/architecture combinations. Select a framework only after its
   retained/immediate/declarative/WebView model and deployment constraints fit
   those flows.
3. Keep a framework-independent model, use cases, validation, and error
   classification outside the view layer. Translate user input into messages or
   commands at the UI edge; render derived state rather than letting widgets own
   business rules.
4. Treat the GUI event loop and UI objects as thread-affine. Do not block the
   UI thread with I/O, CPU-heavy work, locks, sleeps, or synchronous process
   calls. Give every worker an owner, bounded queue, cancellation/shutdown path,
   and typed result message; marshal results back through the framework's
   documented UI/event-loop mechanism.
5. Compose layouts for keyboard and resize behavior first, then apply tokens or
   named theme roles consistently. Use semantic labels, focus order, visible
   focus indication, keyboard alternatives, text scaling, contrast, and native
   accessibility semantics; test the actual OS behavior instead of assuming a
   widget exposes it.
6. Keep user-facing errors actionable and safe. Preserve structured context for
   diagnostics without displaying internal paths, commands, secrets, or raw
   backend failures. Add cancellation, loading, empty, retry, and failure states
   for long-running operations.
7. Test pure state transitions and use cases without a GUI runtime; add focused
   framework, accessibility, and platform smoke tests only where they prove
   behavior the lower layer cannot. Reproduce active GUI, rendering, event-loop,
   or packaging failures before changing code.
8. Build and smoke-test release artifacts on each claimed Windows, macOS, and
   Linux target. Verify signing, notarization, installers, bundle metadata,
   native dependencies, WebView/runtime prerequisites, update behavior, and
   permissions from the target repository's release requirements.

## Desktop Boundaries

- Make the main/event-loop thread the sole owner of windows, widgets, renderer
  state, and other framework objects unless the selected framework documents a
  different rule. Pass owned data or messages across threads, not UI references.
- Prefer explicit operation IDs or state generations when results can arrive
  out of order. Ignore superseded results and clean up workers/subscriptions on
  window close, navigation, or application shutdown.
- Model modal dialogs, file pickers, clipboard, notifications, deep links,
  global shortcuts, files, processes, and OS permissions as boundary adapters.
  Validate untrusted input and retain platform-specific behavior at that edge.
- Do not promise native accessibility, performance, portability, or installer
  parity from a framework name or a successful build. Verify the pinned
  framework, renderer, target triple, desktop environment, and release artifact.

## Framework-Specific Work

- **Iced:** use the Elm-style `State`/`Message`/`update`/`view` boundary;
  represent async actions as `Task`s and long-lived event sources as
  `Subscription`s. See the selection reference and, for 0.14.x only, the dated
  [Iced reference](references/iced-0.14.md).
- **egui/eframe:** immediate-mode UI is rebuilt each frame. Keep state durable,
  make frame work bounded, and compose UI through small `App::ui`-style methods
  called by the resolved eframe `App` callback.
- **Slint:** keep declarative `.slint` components, properties, callbacks, and
  generated bindings separate from Rust application services; schedule
  cross-thread UI work through its documented event-loop bridge.
- **Tauri:** treat Rust commands, IPC payloads, capabilities/permissions, and
  WebView content as explicit trust and compatibility boundaries. Do not treat
  a WebView message as a local Rust function call.

## Strict Routing

- Load [`rust-engineering`](../rust-engineering/SKILL.md) for Cargo, ownership,
  types, traits, errors, modules, features, and ordinary Rust implementation.
- Load [`rust-async-web`](../rust-async-web/SKILL.md) for Tokio, async runtime,
  cancellation, channels, and async task mechanics; load
  [`parallelism-engineering`](../parallelism-engineering/SKILL.md) when a
  CPU-bound workload needs a parallel decomposition rather than routine GUI
  background work.
- Load [`rust-testing-quality`](../rust-testing-quality/SKILL.md) for Rust test
  design and Cargo quality lanes; load [`test-driven-development`](../test-driven-development/SKILL.md)
  for an implementation TDD loop and [`systematic-debugging`](../systematic-debugging/SKILL.md)
  for an active unexplained failure.
- Load [`javascript-typescript-engineering`](../javascript-typescript-engineering/SKILL.md)
  for Tauri frontend JavaScript/TypeScript, [`css-scss-styling`](../css-scss-styling/SKILL.md)
  for WebView styling, and [`ux-accessibility-review`](../ux-accessibility-review/SKILL.md)
  for a requested UI/accessibility audit.
- Load [`api-design`](../api-design/SKILL.md) for published IPC, plugin, deep-link,
  or SDK contracts; use [`security-review`](../security-review/SKILL.md) for
  commands, capabilities, permissions, file/process/URL handling, IPC, secrets,
  or other trust boundaries; use [`threat-modeling`](../threat-modeling/SKILL.md)
  for new boundary design.
- Load [`dependency-supply-chain-review`](../dependency-supply-chain-review/SKILL.md)
  for crate, lockfile, plugin, installer, native dependency, signing, or
  provenance risk. Load [`ci-release-engineering`](../ci-release-engineering/SKILL.md)
  for checked-in CI/release automation and [`documentation-engineering`](../documentation-engineering/SKILL.md)
  for reader-facing documentation.
- Load [`webassembly-engineering`](../webassembly-engineering/SKILL.md) only
  when the desktop product actually includes a Wasm host/guest or component
  boundary. A Tauri WebView alone is not that boundary.

## Completion Checks

- UI state, event-loop ownership, worker lifecycle, cancellation, and shutdown
  are explicit and testable.
- Keyboard, focus, labels, resize, theme/contrast, loading, and error paths are
  covered at the lowest useful level and on affected native platforms.
- Release bundles/installers are tested on every claimed OS/architecture with
  required platform metadata, signing/notarization, and dependencies verified.
- Framework guidance and API calls were checked against the resolved version,
  not an unversioned current documentation page.

## Tauri Zod Routing

For Tauri WebView-side parsing and shared malformed-IPC fixtures, load [`zod-engineering`](../zod-engineering/SKILL.md). Rust retains serde DTO validation, capability/authorization checks, and published IPC ownership.
