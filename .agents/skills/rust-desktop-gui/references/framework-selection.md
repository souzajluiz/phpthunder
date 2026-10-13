# Rust Desktop GUI Framework Selection

Choose from the target repository's requirements and resolved versions, not a
framework's current marketing or an unpinned example. Make a small vertical
slice that proves the required widgets, accessibility, renderer, OS targets,
native integrations, and release artifact before a migration or broad adoption.

| Framework | Choose when | Core model and boundary | Do not infer |
| --- | --- | --- | --- |
| **Iced** | A Rust-first, retained-state GUI benefits from an Elm-style message loop and declarative view. | `State` plus `Message`; `update` changes state and returns on-demand `Task`s; `view` renders state; `Subscription`s declare ongoing external event sources. | Exact widget/accessibility coverage, renderer/backend support, or API compatibility across versions. |
| **egui/eframe** | Tooling, editors, visualizers, or highly interactive UIs benefit from immediate-mode composition and fast iteration. | UI is rebuilt every frame from durable app state. Implement the resolved eframe `App` callbacks and delegate rendering to small project-local helpers. Keep costly work and side effects out of frame rendering. | Retained-widget lifecycle, automatic persistence, native platform behavior, or a particular eframe callback signature. |
| **Slint** | A declarative component language, explicit properties/callbacks, and a UI/Rust model split fit the product and toolchain. | Define UI in `.slint` components; bind properties and callbacks to Rust services; run the selected main event loop. Background threads schedule UI changes with the documented `invoke_from_event_loop` bridge rather than touching components directly. | That generated bindings, the event-loop API, native style, or target support is identical across Slint releases. |
| **Tauri** | A native Rust host plus an existing web frontend, WebView platform, and IPC-capability model are intentional requirements. | Rust is the host; HTML/CSS/JS renders in a system WebView; commands/events form IPC; capabilities scope what WebView code may do. Validate and authorize every command and payload at the host boundary. | That WebView content is trusted, capabilities replace input validation, all OS WebViews behave alike, or the app is a pure native-widget GUI. |

## Decision Rules

1. Keep the selected framework's event loop at the process edge. Application
   logic should accept typed inputs and return typed outputs without window,
   widget, renderer, DOM, or WebView handles.
2. Select **Iced** for the message/update/view model, **egui/eframe** for an
   immediate-mode frame model, **Slint** for a `.slint` declarative component
   model, or **Tauri** when a WebView and web frontend are deliberate parts of
   the desktop architecture. These are architectural choices, not interchangeable
   widget libraries.
3. For Tauri, define IPC command/event schemas, authorization, origin/content
   policy, capability scope, and error mapping before implementation. For the
   other frameworks, make UI-to-application message/command conversion equally
   explicit.
4. Confirm the exact pinned framework release, enabled features, renderer, Rust
   MSRV, target triples, system libraries, and supported desktop environments.
   Consult that release's official documentation, not a mutable “latest” URL.
5. Validate the release bundle on each claimed Windows, macOS, and Linux target.
   Cross-compilation or a local development window does not prove deployment,
   accessibility, code signing, notarization, WebView availability, or installer
   behavior.

## Non-Selection Boundaries

- A browser-only frontend is outside this skill unless it is the WebView portion
  of a Tauri desktop application.
- Use a dedicated framework's current, version-pinned reference for APIs; this
  document deliberately does not reproduce mutable API details.
- Route Cargo/Rust mechanics, async design, security, testing, release
  automation, and WebView frontend implementation to the corresponding skills
  named in [the parent skill](../SKILL.md#strict-routing).
