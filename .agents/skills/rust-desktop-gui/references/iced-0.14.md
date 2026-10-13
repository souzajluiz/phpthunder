# Iced 0.14.0 Reference

**Checked: 2026-08-01. Version: iced 0.14.0.** This is a release-specific
reference, not evidence that `iced` 0.14.0 is current or compatible with a
target project.

> **Verify the target version first.** Read the resolved `Cargo.lock` and
> `Cargo.toml` features, then use the exact release's official API docs. Do not
> copy this example into Iced 0.13, a newer Iced release, or an unpinned project.
> APIs, features, renderers, platform support, and accessibility behavior can
> change between releases.

## 0.14.0 Sources

- [Iced 0.14.0 GitHub release](https://github.com/iced-rs/iced/releases/tag/0.14.0)
- [0.14.0 `iced::application` API](https://docs.rs/iced/0.14.0/iced/fn.application.html)
- [0.14.0 `Application` builder API](https://docs.rs/iced/0.14.0/iced/application/struct.Application.html)
- [0.14.0 `Task` API](https://docs.rs/iced/0.14.0/iced/struct.Task.html)
- [0.14.0 `Subscription` API and lifecycle](https://docs.rs/iced/0.14.0/iced/struct.Subscription.html)

These links are intentionally version-labelled and release-specific; do not
replace them with a mutable “latest” documentation URL.

## Minimal 0.14 Builder Shape

```rust
use iced::widget::{button, column, text};
use iced::{Element, Subscription, Task};

#[derive(Default)]
struct State {
    count: u64,
}

#[derive(Debug, Clone)]
enum Message {
    Increment,
}

fn main() -> iced::Result {
    iced::application(State::default, update, view)
        .subscription(subscription)
        .run()
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Increment => state.count += 1,
    }

    Task::none()
}

fn view(state: &State) -> Element<'_, Message> {
    column![
        text(state.count),
        button("Increment").on_press(Message::Increment),
    ]
    .into()
}

fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::none()
}
```

`iced::application(State::default, update, view)` initializes state, routes
messages to `update`, and renders with `view`. Configure title, theme, window,
font, settings, and other builder options only after checking their 0.14.0 docs
and the target's enabled features.

## Task and Subscription Lifecycle

- Return a `Task<Message>` from initialization or `update` for finite,
  on-demand concurrent work. `Task::perform`, `Task::run`, and composition APIs
  produce messages that re-enter `update`; return `Task::none()` when no work is
  requested. Give each operation a state owner and represent success, failure,
  cancellation, and stale result handling explicitly.
- Return `Subscription<Message>` from the application's `subscription` function
  for passive, ongoing sources such as timers, input, or connections. A
  subscription has no effect until returned to the runtime. The runtime starts a
  newly returned subscription, tracks it by identity, and kills its stream when
  it stops being returned. Return `Subscription::none()` when disabled and keep
  identity stable for a stream that should continue.
- Do not use a recurring `Task` as an unmanaged background worker or assume that
  dropping UI state handles cancellation correctly. Prefer the documented
  subscription lifecycle for ongoing sources, model worker shutdown, and verify
  the precise selected-version cancellation behavior.

Keep `State`, `Message`, `update`, and `view` small enough to test state
transitions independently. Put blocking or CPU-heavy work behind a bounded
background boundary and return only owned messages to the Iced runtime.

For general Rust, async, security, test, packaging, and release mechanics, use
[the parent skill's strict routing](../SKILL.md#strict-routing) rather than
duplicating those workflows here.
