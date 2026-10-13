---
name: rust-project-structure
description: Conventions for organising modules, files, and folders inside a Rust project. Use when creating new modules, restructuring existing code, or reviewing folder layout in layered Rust applications.
license: UNLICENSED
metadata:
  author: Cristian
  version: "0.0.6"
---

# Project Structure Skill

## Purpose

This skill provides conventions for organising modules inside `src/`. It covers concept-based folder layout, internal file naming, trait and error placement, and layer-specific adaptations. These rules apply on top of a layered architecture (e.g. hexagonal / ports-and-adapters) and complement code-style and design-principle guidelines.

## When to Apply

Apply these guidelines when:
- Creating new modules or concept folders
- Restructuring or reorganising existing source code
- Adding new components (services, orchestrators, executors, providers) to a concept
- Reviewing folder layout for consistency
- Working in a layered Rust codebase (domain / application / infrastructure or similar)

## When NOT to Apply

Do not apply when:
- The project is a flat, single-file utility or script
- Working on a library crate with no layered architecture
- The project already follows a different, well-established convention that the team has agreed on

## Core Principles

### 0. Modern Module Files: Never `mod.rs` (CRITICAL)

`mod.rs` is legacy Rust module layout and must never be created or preserved. Use the modern sibling-file layout: `module_name.rs` next to `module_name/`, with submodules declared from `module_name.rs`.

There is no exception, including under `tests/`. A directory shared by two test crates is reached with `#[path = "common/<file>.rs"] mod <name>;` from each entry point that needs it (rust-testing Section 15) — that needs no `mod.rs`. Treat any existing or proposed `mod.rs` as a structural issue to be converted to `module_name.rs` before proceeding.

```
# Bad — legacy module layout
src/application/order/
├── mod.rs
├── service.rs
└── port.rs

# Good — modern sibling-file layout
src/application/
├── order.rs
└── order/
    ├── service.rs
    └── port.rs
```

What goes *inside* the sibling module file is fixed by principle 14: module docs, private `mod` declarations, and glob re-exports — nothing else.

### 1. Concept-Based Folders (CRITICAL)

Every top-level folder inside a layer represents a **business concept** or a clearly named shared capability (e.g. `order/`, `payment/`, `notification/`, `encryption/`). Never create folders named after a component role (e.g. ~~`orchestrator/`~~, ~~`executor/`~~, ~~`scheduler/`~~, ~~`handler/`~~).

If a concept needs an orchestrator, executor, or scheduler, that component lives **inside** the concept folder.

```
# Bad — folders named after roles
src/application/
├── orchestrators/
│   ├── order.rs
│   └── payment.rs
├── executors/
│   ├── order.rs
│   └── payment.rs
└── services/
    ├── order.rs
    └── payment.rs

# Good — folders named after concepts
src/application/
├── order/
│   ├── port.rs
│   ├── service.rs
│   ├── orchestrator.rs
│   └── executor.rs
└── payment/
    ├── port.rs
    ├── service.rs
    └── orchestrator.rs
```

Nested folders are allowed when they model a focused sub-capability of the parent concept (e.g. `destination/file/`). A sub-capability follows the same local structure rules as any other concept.

### 2. Internal File Layout (CRITICAL)

Each concept folder follows this canonical structure:

| File | Contains | Required? |
|------|----------|-----------|
| `port.rs` | **All** traits (ports) for this concept — services, orchestrators, executors, providers | Yes, when this layer defines local traits for the concept |
| `service.rs` | `Default*` implementation of the main service trait | Only if the concept has a main service impl |
| `error.rs` | **All** error enums for this concept — service errors, orchestrator errors, execution errors — plus any error-translation code this layer owns for the concept (`From` impls, mapping functions) | Only if the concept defines concept-local errors or this layer translates the concept's errors |
| `model.rs` | The concept's **dataflow values** — the value structs/enums a port takes as input or returns as output, plus internal value objects. Not construction-time config (see `config.rs`) | Only if the concept defines such data types |
| `config.rs` | Construction-time tunables/policy structs for the concept's components — injected once through a constructor, never carried by a port method | Only if the concept defines such tunables |
| `dto.rs` | Wire-transfer shapes serialized across the infrastructure/API boundary (HTTP/gRPC), when one file holds all of a concept's wire types; prefer per-direction `request.rs`/`response.rs` | No — never in domain or application |
| `orchestrator.rs` | `Default*Orchestrator` implementation | No |
| `executor.rs` | `Default*Executor` implementation | No |
| `provider.rs` | `Default*Provider` implementation | No |
| `provisioner.rs` | `Default*Provisioner` implementation | No |
| `sweeper.rs` | Periodic cleanup runner with a `tick()` method | No |
| `manager.rs` | Periodic enforcement runner with a `tick()` method | No |
| Other files | Concept-specific helpers (e.g. `progress.rs`, `validation.rs`) | No |

Module-level `const` items go in the file that owns their meaning: a tunable's default value in `config.rs` beside the struct it parameterizes, a constant tied to a dataflow value (a wire discriminator, a display label) in `model.rs` next to its type, and a constant used by a single file in that file. Never create a `constants.rs` bucket.

### 3. One Trait File Per Concept (CRITICAL)

If a layer defines traits for a concept, **all** of them go in `port.rs`. Do not split traits across `port.rs`, `orchestrator.rs`, `handler.rs`, etc. Do not create placeholder `port.rs` files in layers that only implement traits defined elsewhere.

```rust
// Good — all traits for the concept in one file
// src/application/order/port.rs

pub trait OrderService: Clone + Send + Sync + 'static {
    fn create_order(
        &self,
        request: &CreateOrderRequest,
    ) -> impl Future<Output = Result<Order, CreateOrderError>> + Send;
}

pub trait OrderExecutor: Clone + Send + Sync + 'static {
    fn execute(
        &self,
        order: &Order,
    ) -> impl Future<Output = Result<OrderResult, ExecuteOrderError>> + Send;
}

pub trait OrderNotifier: Clone + Send + Sync + 'static {
    fn notify_completed(
        &self,
        order: &Order,
    ) -> impl Future<Output = Result<(), NotifyError>> + Send;
}
```

### 4. One Error File Per Concept (HIGH)

All error enums for a concept go in `error.rs`. Do not create `orchestrator_error.rs`, `executor_error.rs`, etc.

```rust
// Good — all errors for the concept in one file
// src/application/order/error.rs

#[derive(Debug)]
pub enum CreateOrderError {
    Duplicate { name: OrderName },
    InvalidInput(String),
    Unknown(String),
}

#[derive(Debug)]
pub enum ExecuteOrderError {
    NotFound { id: OrderId },
    AlreadyExecuted { id: OrderId },
    Unknown(String),
}
```

`error.rs` also owns **error translation**. A layer that defines no error enums of its own but converts another layer's errors — an infrastructure adapter mapping `sqlx::Error` into an application error, an API module mapping an application error into an `ApiError` — puts those `From` impls and mapping functions in the concept's `error.rs` within that layer. Classifying what a foreign error means for the concept is a single responsibility; give it a single home.

Three rules follow:

- An inline `map_err` closure inside one adapter is fine while the mapping is used only in that file.
- The moment two files in the same concept folder need the same translation (same source error, same target type, same policy), extract it into the concept's `error.rs` in that layer. Never leave identical private mapping functions duplicated across sibling adapter files.
- Never create `mapper.rs` (or `conversion.rs`, `convert.rs`) for error translation. "Mapper" is a weak role-bucket name that attracts unrelated mapping code — row-to-model mapping belongs next to the row struct inside the adapter file, not in a shared mapper. `error.rs` names the responsibility precisely.

Do not move technology-error translation inward next to the enum definition: a `From<sqlx::Error>` impl in `application/order/error.rs` would leak the database crate into the application layer.

```rust
// Good — shared translation policy in the concept's error.rs of the layer that owns it
// src/infrastructure/database/order/error.rs

use crate::application::order::OrderReadError;
use crate::infrastructure::database::sqlstate::is_undefined_table;

/// Classifies what a database error means for order reads.
/// Shared by every order read adapter in this folder.
pub(super) fn map_read_error(error: sqlx::Error) -> OrderReadError {
    if is_undefined_table(&error) {
        OrderReadError::ProjectionUnavailable
    } else {
        OrderReadError::Database(error.to_string())
    }
}
```

### 5. Naming: Trait Gets the Clean Name, Struct Gets the Prefix (HIGH)

The trait (port) gets the clean, unqualified name (`OrderService`). The concrete struct gets a descriptive prefix (`DefaultOrderService`).

If a layer contains **multiple concrete adapters** for the same upstream trait, use an explicit adapter name based on the external system or protocol (`PostgresOrderRepository`, `RedisOrderCache`) instead of `Default*`.

```rust
// Single canonical implementation
pub trait OrderService { /* ... */ }            // trait — clean name
pub struct DefaultOrderService { /* ... */ }    // struct — Default prefix

// Multiple adapters for the same trait
pub trait OrderRepository { /* ... */ }         // trait — clean name
pub struct PostgresOrderRepository { /* ... */ } // struct — technology prefix
pub struct SqliteOrderRepository { /* ... */ }   // struct — technology prefix
```

### 6. Files Named After the Role, Not the Prefix (HIGH)

The service impl goes in `service.rs`, the orchestrator impl goes in `orchestrator.rs`, the executor impl goes in `executor.rs`. Do not use `default_orchestrator.rs` — the `Default` prefix belongs on the struct, not the file.

```
# Bad
src/application/order/
├── default_service.rs
├── default_orchestrator.rs
└── default_executor.rs

# Good
src/application/order/
├── service.rs          # contains DefaultOrderService
├── orchestrator.rs     # contains DefaultOrderOrchestrator
└── executor.rs         # contains DefaultOrderExecutor
```

### 7. Short Module Names — Let the Parent Provide Context (HIGH)

When a file lives inside a concept folder, its name should not repeat the concept. Use `handler.rs` inside `order/`, not `order_handler.rs` — the full path `order::handler` already reads clearly. The struct inside keeps its full descriptive name (`OrderHandler`).

```
# Bad — stuttering names
src/infrastructure/api/order/
├── order_handler.rs
├── order_request.rs
└── order_response.rs

# Good — parent provides context
src/infrastructure/api/order/
├── handler.rs          # contains OrderHandler
├── request.rs          # contains CreateOrderRequest, UpdateOrderRequest
└── response.rs         # contains OrderResponse
```

### 8. NoOp Stubs Live Close to the Trait (MEDIUM)

A NoOp/stub implementation goes in `port.rs` next to the trait it stubs; if the concept already has the matching role file (e.g. `provider.rs` for a `NoOpOrderProvider`), it may go there instead. Do not hide a provider stub in `service.rs` and do not create separate `noop_*.rs` files.

### 9. Cross-Cutting Components Need a Capability Name (MEDIUM)

If multiple concepts share infrastructure, extract a shared module named after the **capability** it exposes, never after the consuming role.

```
# Bad — named after the role
src/application/executor/
    encryption.rs    # used by order executor and payment executor

# Good — named after the capability
src/application/encryption/
    port.rs
    service.rs
```

### 10. Concept Folders Without a Service or Port Are Suspect (MEDIUM)

If a folder has no `service.rs` or `port.rs` (e.g. a `lock/` folder with only `sweeper.rs`), consider whether the sweeper belongs inside the concept it cleans up (e.g. the `schedule/` concept for schedule lock sweeping).

### 11. Data Types Live in `model.rs`, Not `port.rs` (CRITICAL)

`port.rs` holds traits only and `error.rs` holds error enums and error translation only (principles 3–4). Every concept-local **dataflow value** — the value structs and enums a port takes as input or returns as output, plus internal value objects — goes in `model.rs` (singular). Do not leave data structs in `port.rs`. Construction-time tunables are not dataflow and go in `config.rs` instead (principle 13).

Never use `types.rs`, `data_structs.rs`, or pluralised names (`models.rs` / `dtos.rs`). `dto.rs` is reserved for wire-transfer shapes that serialize across the infrastructure/API boundary (HTTP/gRPC request/response); prefer `model.rs` for types that stay within a layer. The one exception is primitive types shared across sibling sub-capabilities, which live in a `primitives.rs` at the capability parent (see principle 12).

```rust
// Bad — a data struct in the traits file
// src/application/notification/port.rs
pub trait ContextResolver { /* ... */ }
pub struct SubjectDisplayContext { /* data — belongs in model.rs */ }

// Good — traits in port.rs, data in model.rs
// src/application/notification/port.rs
pub trait ContextResolver { /* ... */ }

// src/application/notification/model.rs
pub struct SubjectDisplayContext { /* ... */ }
```

### 12. Consolidate Sibling Modules That Share a Behaviour Contract (MEDIUM)

When several peer modules drive the same family of artefacts, share the same primitive types, **and** follow the same lifecycle policy (retry/backoff, attempt limits, reporting), collapse them under one capability-named parent. Each peer becomes a technology-named subfolder; the shared primitive types live in a sibling `primitives.rs`. The parent name carries the capability, so the subfolders drop the redundant suffix.

```
# Bad — three sibling modules that all do "cleanup", each duplicating the primitives
src/application/
├── cleanup_sweeper.rs        # SweepReport, backoff policy
├── kubernetes_cleanup/       # k8s orphan queues
└── database_cleanup/         # db orphan queue

# Good — one capability parent, technology subfolders, primitives in one place
src/application/cleanup/
├── primitives.rs             # shared SweepReport, backoff policy
├── kubernetes/
└── database/
```

Propagate the rename symmetrically through every layer that mirrors the module path (e.g. `infrastructure/runtime/cleanup/{kubernetes,database}`, `tests/.../cleanup/{kubernetes,database}`). Function and struct names whose words happen to overlap the old path describe what they do, not where they live — leave them unchanged.

### 13. Construction-Time Config Lives in `config.rs`, Not `model.rs` (HIGH)

`model.rs` holds dataflow values — types that move through port methods at request time. A tunables/policy struct (backoff windows, attempt caps, tick intervals, retention windows) has a different lifecycle: it is built once at startup, injected through a component's constructor, and its consumer is the composition root — not port callers. That is a separate responsibility; give it a separate home: the concept's `config.rs`.

The litmus test: **a type that appears in any port method signature goes in `model.rs`; a type that never appears in a port signature and is built once at startup — via `new()`, a builder, or a factory function — and injected into a component goes in `config.rs`.** When both hold, the port side wins: a type that crosses a port at request time is dataflow even if it is also constructor-injected. Apply the test, not the name — a settings type that a port fetches or updates at request time stays in `model.rs` even if it is called "config".

Defining the struct in the application (or domain) layer is correct and this rule does not change it: the layer that consumes the policy defines its shape; infrastructure reads the environment and constructs the value, so the dependency still points inward. Never use `settings.rs`, `tunables.rs`, or `options.rs` for this file.

A config aggregate that no single concept owns — a parameter object grouping several concepts' tunables so the wiring factory's signature stays stable — is wiring, not application logic: it belongs to the composition root's input bundles (see the composition-root pattern), never inside a concept folder or a layer's module file. If application code consumes a config struct directly, it is concept config — define it in that concept's `config.rs`, and split an aggregate that spans several concepts instead of sharing one struct between application code and wiring.

```rust
// Bad — construction-time tunables buried among dataflow values
// src/application/order/model.rs
pub struct CreateOrderRequest { /* port input — belongs here */ }
pub struct OrderSweeperConfig { /* constructor input — does not */ }

// Good — dataflow in model.rs, injected policy in config.rs
// src/application/order/config.rs

/// Tunables for the order outbox sweeper: retry backoff, attempt cap,
/// tick interval. Built by infrastructure from the environment and
/// injected by the composition root.
pub struct OrderSweeperConfig {
    pub backoff_seconds: u64,
    pub max_attempts: u32,
    pub sweep_interval_seconds: u64,
}
```

### 14. The Concept Module File Is a Facade: Private `mod` + Glob Re-Exports (HIGH)

The sibling module file that fronts a concept folder (`order.rs` next to `order/`) contains exactly three kinds of lines: module docs (`//!`), a private `mod` declaration per file in the folder, and one glob re-export per file. No `pub mod` for files, no curated per-item re-export lists, no inline items, no logic.

```rust
// src/application/order.rs — the whole file
//! Order management: creation, execution, notification.

mod config;
mod error;
mod model;
mod port;
mod service;

pub use config::*;
pub use error::*;
pub use model::*;
pub use port::*;
pub use service::*;
```

This makes the concept path the **single canonical import path**: `use crate::application::order::OrderService;` compiles; `use crate::application::order::port::OrderService;` does not — the private `mod` blocks it. Two problems disappear at once:

- **File placement stays correctable.** Which file a type lives in (`port.rs` vs `model.rs` vs `config.rs`) never appears in a consumer's import, so moving a type between files — fixing a placement mistake — touches nothing outside the concept folder.
- **No import drift.** When both a deep path and a re-export compile, a codebase splits between them and no tool can enforce the choice. When only one path compiles, the compiler is the enforcement.

Item visibility is the export list — a glob never widens visibility:

| Marker on the item | Effect through the glob |
|---|---|
| `pub` | On the concept surface: other layers, `tests/`, downstream crates |
| `pub(crate)` | Reachable via the concept path inside the crate; invisible to `tests/` and downstream crates |
| private / `pub(super)` | Concept-internal; not re-exported |

Two files exporting the same `pub` name collide at compile time — rename one; concept-scoped type names (principles 5 and 7) make this rare.

Scope: **files are hidden behind the facade; folders remain addressable namespaces.** A nested sub-capability folder (principle 1) is declared `pub mod` — `backup.rs` declares `pub mod replica;` so `backup::replica::…` keeps its namespace — and the sub-capability's own module file follows this same facade rule for its files. Layer module files (`application.rs`) and adapter-family files (`infrastructure/database.rs`) declare their child modules with `pub mod` and add **no per-item re-exports**: a layer-level re-export would reintroduce the second import path the facade just eliminated.

Crate scope: this principle is for **application crates** — services, binaries, and workspace members consumed only inside the repository, where the compiler sees every consumer and a collision is an instant build error. In a **published library crate**, the module hierarchy is legitimately part of the public API (`std::collections::hash_map`, `tokio::sync::mpsc`): keep meaningful `pub mod` namespaces and curate the root surface with explicit re-exports instead. A glob facade is a semver hazard there — every new `pub` item silently joins the public API, and an added name can collide with a downstream glob import without your own build ever failing.

## Infrastructure Layer Adaptations

The infrastructure layer follows the same core principles, with these additional allowances:

### Adapter-Family Folders at the Top Level

Infrastructure may use **adapter-family** or **technical-capability** folders at the top level when that is the cleanest primary axis. Examples: `api/`, `database/`, `gateway/`, `runtime/`, `bootstrap/`, `session/`, `crypto/`, `config/`, `event/`.

Inside an adapter family, the next level is organized **by concept, never by role**: `api/order/`, `database/order/`, `gateway/source/` — never `api/handler/` or `api/model/` folders holding one file per concept (that transposes the concept × role matrix onto the wrong axis; principle 1 applies inside families too). Family-wide helpers with no concept owner (a `router.rs`, `pagination.rs`, the family's `error.rs`) sit directly in the family folder.

Two of these have fixed meanings: use `session/` for token/session lifecycle infrastructure, and `crypto/` for low-level cryptographic primitives such as hashing and encryption.

```
src/infrastructure/
├── api/                    # HTTP adapter family
│   ├── order/
│   │   ├── handler.rs
│   │   ├── request.rs
│   │   └── response.rs
│   └── payment/
│       ├── handler.rs
│       └── response.rs
├── database/               # Persistence adapter family
│   ├── order/
│   │   └── repository.rs   # PostgresOrderRepository
│   └── payment/
│       └── repository.rs
├── gateway/                # External system adapters
│   ├── source/
│   │   └── stripe.rs
│   └── destination/
│       └── s3.rs
├── runtime/                # Long-lived loop wrappers
│   └── order/
│       └── sweeper.rs
└── bootstrap/              # Startup-only initialization
    └── seeder.rs
```

### Avoid Weak Folder Buckets

Avoid generic folders like `provider/`, `handler/`, `repository/`, `service/`, `shared/`, or `common/` that merely collect unrelated concerns by role. Prefer stronger adapter boundaries (`api/`, `database/`) or named capabilities (`credentials/`, `multipart_upload/`).

### Runtime and Bootstrap Separation

- Put long-lived loop wrappers and background runners under `runtime/`, grouped by the concept they drive.
- Put startup-only seeders, bootstrap coordinators, and one-shot initialization tasks under `bootstrap/`.

### Missing `port.rs` Is Natural in Infrastructure

Infrastructure often implements ports defined in `domain/` or `application/` rather than defining new local traits. Missing `port.rs` should not be treated as a structural violation.

### Technology-Named Modules for Single Providers

When a capability currently has a single provider-specific implementation, naming the top-level folder after the external system is acceptable (e.g. `minio/`, `stripe/`). Revisit when a broader capability folder would clearly improve clarity.

### Shared Helpers in the Smallest Owning Namespace

Place shared helpers in the smallest meaningful owning namespace. If only Kubernetes source adapters use a helper, keep it under `gateway/source/kubernetes/`. If multiple destination adapters use an I/O helper, `gateway/destination/io.rs` is better than `gateway/destination/shared/`.

### Split by Variant vs. Consolidate

When one port has several provider variants, choose the file layout by how many there are and how much they diverge:

- **Few and uniform** → keep all variants in one file (e.g. `connectivity.rs`). Splitting adds noise rather than clarity.
- **Many or divergent** → one file per variant in a folder (e.g. `connectivity/postgres.rs`, `connectivity/redis.rs`), with a dispatcher in `provider.rs` that selects and calls the right one.

Pick a threshold and apply it consistently — a useful default is to flip from one-file to per-variant files once variants exceed ~5. Asymmetry between two adapter families is fine when their variant counts justify it; note the reason where it is not obvious.

### Adaptations Do Not Relax the File-Role Rule

None of the allowances above override principle 6. A file that contains a provider, provisioner, service, executor, or orchestrator is still named after that role — unless it is intentionally a focused concept-specific helper.

## Anti-Patterns to Avoid

1. **`mod.rs` files**: Creating or preserving legacy Rust module layout instead of `module_name.rs` — no exceptions, `tests/` included
2. **Role-based folders**: Grouping by `services/`, `handlers/`, `repositories/` instead of business concepts
3. **Stuttering module names**: `order/order_handler.rs` instead of `order/handler.rs`
4. **Scattered traits**: Splitting traits for a single concept across multiple files
5. **Scattered errors**: Creating `service_error.rs`, `executor_error.rs` instead of a single `error.rs`
6. **Weak bucket folders**: `shared/`, `common/`, `utils/` collecting unrelated concerns
7. **File names matching struct prefixes**: `default_service.rs` instead of `service.rs`
8. **Orphan concept folders**: Folders with only a sweeper or runner and no port or service
9. **NoOp files**: Separate `noop_provider.rs` files instead of keeping stubs near the trait
10. **Data structs in `port.rs`**: Putting value structs/enums in the traits file instead of `model.rs` (dataflow values) or `config.rs` (construction-time tunables)
11. **Parallel sibling modules**: Several peer modules doing the same thing with duplicated primitive types and lifecycle policy, instead of one capability parent with technology subfolders and a shared `primitives.rs`
12. **Scattered or bucketed error translation**: Identical error-mapping functions copy-pasted across sibling adapter files, or collected in a weak `mapper.rs`, instead of one function in the concept's `error.rs`
13. **Config structs in `model.rs`**: Construction-time tunables mixed into the dataflow-values file instead of the concept's `config.rs`
14. **Public submodule files or deep imports**: `pub mod port;` on a concept's files, curated per-item re-export lists, or a consumer importing `order::model::X` — in an application crate the concept module file is a glob facade and the concept path is the only import path (a published library legitimately keeps `pub mod` namespaces and a curated surface — see principle 14's crate scope)

## Quick Reference

### Adding a New Concept

1. Create `concept.rs` next to `concept/`; never create `concept/mod.rs`. It holds only module docs, private `mod` declarations, and `pub use <file>::*;` globs (principle 14)
2. Create a folder named after the business concept under the appropriate layer
3. Add `port.rs` with all traits for this concept (if defining traits)
4. Add `error.rs` with all error enums (if defining errors)
5. Add `model.rs` for port dataflow values and `config.rs` for construction-time tunables, only as needed
6. Add `service.rs` with the `Default*` implementation
7. Add additional role files (`orchestrator.rs`, `executor.rs`) only as needed

### Adding a Component to an Existing Concept

1. If the component introduces new traits, add them to the existing `port.rs`
2. If the component introduces new errors, add them to the existing `error.rs`
3. If the component introduces construction-time tunables (backoff, intervals, attempt caps), add them to the concept's `config.rs`
4. Create a new file named after the role (e.g. `executor.rs`)
5. Register it in the concept module file: `mod executor;` plus `pub use executor::*;` (principle 14)
6. Name the struct with the concept prefix and role suffix (`OrderExecutor` trait, `DefaultOrderExecutor` struct)

### Adding an Infrastructure Adapter

1. Place it under the appropriate adapter-family folder (`api/`, `database/`, `gateway/`)
2. Organize by concept inside the adapter family
3. Use technology-specific struct names (`PostgresOrderRepository`, not `DefaultOrderRepository`)
4. No need for a `port.rs` — the trait lives in domain or application
5. Map technology errors inline in the adapter while only that file needs the mapping; once sibling adapters share the same translation, move it to the concept's `error.rs` — never a `mapper.rs`
