# Technical Specification

## Development of the `boa-sw` crate family — a Service Workers implementation for the Boa JavaScript engine

| | |
|---|---|
| **Document** | Technical specification for a software component (TS / *техническое задание*) |
| **Product** | `boa-sw` — W3C Service Workers implementation for the embeddable JavaScript engine Boa |
| **TS version** | 1.0 |
| **Date** | 2026-09-04 |
| **Target platform** | Rust 1.91.0 (edition 2024), `boa_engine` 0.22.x |
| **Normative specification** | [Service Workers, W3C Candidate Recommendation Draft](https://www.w3.org/TR/service-workers/) (hereafter **the Spec**) |
| **Sibling projects** | `boa-idb` (IndexedDB), `boa-webstorage` (Web Storage) — same architectural style, same working contract |
| **Status** | for execution |
| **Audience** | Rust engineers **and** LLM coding agents executing one work order at a time |

---

## How to read this document

* Sections 1–14 are **normative**: they define *what* must exist, with exact types, algorithms and rules.
* Section 15 defines **quality gates**.
* Section 16 is the **work breakdown**: milestones `M0…M5` and 27 tasks `T-01…T-27`. Every task is scoped so that a mid-tier coding model can implement it mechanically, without making architectural decisions, and every task carries **explicit, checkable acceptance criteria**.
* Section 17 lists overall acceptance, risks and open questions; section 18 holds the appendices (WebIDL, source tree, SQL schema, acceptance scenario, traceability checklist, work-order template).
* Requirement identifiers have the form `R<section>.<subsection>.<n>` (e.g. `R7.3.1`). Acceptance criteria reference them. Keywords **MUST**, **MUST NOT**, **SHOULD**, **MAY** are used per RFC 2119.
* Spec cross-references are given as fragment anchors of the normative Spec (e.g. `#handle-fetch`). Anchors are indicative; the implementer **MUST** verify the algorithm text against the pinned Spec revision recorded in `docs/SPEC_REVISION.md` (task `T-01`).

---

## Table of contents

1. [General information](#1-general-information)
2. [Target platform, dependencies and project structure](#2-target-platform-dependencies-and-project-structure)
3. [Architecture](#3-architecture)
4. [Host-side Rust API](#4-host-side-rust-api)
5. [JavaScript API surface](#5-javascript-api-surface)
6. [Core model: identifiers, URLs, registrations, scripts](#6-core-model-identifiers-urls-registrations-scripts)
7. [Job queue and lifecycle algorithms](#7-job-queue-and-lifecycle-algorithms)
8. [Fetch interception](#8-fetch-interception)
9. [Cache API](#9-cache-api)
10. [Messaging](#10-messaging)
11. [Integration with the Boa event loop](#11-integration-with-the-boa-event-loop)
12. [Error model and DOMException mapping](#12-error-model-and-domexception-mapping)
13. [Security, privacy, quotas](#13-security-privacy-quotas)
14. [Non-functional requirements](#14-non-functional-requirements)
15. [Testing and quality gates](#15-testing-and-quality-gates)
16. [Work breakdown: milestones and tasks](#16-work-breakdown-milestones-and-tasks)
17. [Overall acceptance, risks, open questions](#17-overall-acceptance-risks-open-questions)
18. [Appendices](#18-appendices)

---

## 1. General information

### 1.1. Purpose and goals

Build a self-contained Cargo workspace that gives a Rust host embedding **Boa** a working **Service Workers** implementation: script registration and versioning, the install/activate lifecycle, network request interception (`FetchEvent`), the Cache API, `postMessage` between clients and workers, and persistence of registrations and caches across process restarts.

The primary consumer is the **BoaX browser-engine project**, where the host owns the networking stack, the document/window ("client") model and the storage layer. Secondary consumers are non-browser hosts that want a Service-Worker-shaped programmable request pipeline (offline app shells, edge-style request handlers, SW test harnesses).

**Key requirement (`R1.1.1`):** JavaScript written for browsers — a typical `service-worker.js` with `install`/`activate`/`fetch`/`message` handlers plus `caches` usage, and a page-side `navigator.serviceWorker.register()` — MUST run unmodified, up to the documented exclusion list in §1.4.

**Secondary requirements:**

* `R1.1.2` — the core (model + algorithms) MUST NOT depend on `boa_engine`; it MUST be testable without a JavaScript engine.
* `R1.1.3` — everything the host owns (network, clients, clock, storage, randomness) MUST be injected through traits; no `std::time::SystemTime::now()`, no direct sockets, no direct filesystem access from core.
* `R1.1.4` — the whole runtime MUST be deterministic under a fixed sequence of host inputs (same inputs ⇒ same event ordering), so that conformance and regression tests are reproducible.
* `R1.1.5` — no `unsafe` code anywhere in the workspace.

### 1.2. Terms and abbreviations

| Term | Definition |
|---|---|
| **The Spec** | W3C Service Workers, revision pinned in `docs/SPEC_REVISION.md` |
| **Boa** | `boa_engine` 0.22.x and companion crates (`boa_gc`, `boa_runtime`, `boa_wintertc`, `boa_macros`, `boa_string`) |
| **Host** | The Rust application embedding Boa and this crate family |
| **Storage key** | Isolation key for storage and registrations; in this project a host-supplied opaque string, normally the serialized origin (§6.1) |
| **Client** | A host-managed execution context that can be controlled by a service worker (a document/window, or a worker). Modelled by `ClientRecord` (§6.1) |
| **Client realm** | The Boa realm in which page/host JavaScript runs and where `navigator.serviceWorker` lives |
| **SW realm** | A dedicated Boa realm created for one running service worker instance; its global is `ServiceWorkerGlobalScope` |
| **Agent** | A logical actor owning one realm and its task queues (§3.4). In v1.0 all agents share one OS thread and one `Context` |
| **Registration** | A `(storage key, scope url)` entry owning up to three workers: installing, waiting, active |
| **Worker record** | Persistent state of one service worker version: script URL, type, state, script resource map |
| **Script resource map** | Map from a script URL to the fetched script bytes plus metadata, used for byte-for-byte update comparison |
| **Job** | An entry of the per-scope job queue: `register`, `update`, or `unregister` (§7.1) |
| **Functional event** | An event dispatched to a service worker that may extend its lifetime (`fetch`, `message`, `install`, `activate`) |
| **ELP** | Extended lifetime promise — a promise passed to `ExtendableEvent.waitUntil()` / `FetchEvent.respondWith()` |
| **WPT** | web-platform-tests, directory `service-workers/` |
| **Work order** | A single task from §16 handed to one implementer/agent |

### 1.3. Scope of work

In scope for version 1.0:

1. **Client-context API**: `ServiceWorkerContainer` (`navigator.serviceWorker`), `ServiceWorkerRegistration`, `ServiceWorker`, `NavigationPreloadManager`.
2. **Worker-context API**: `ServiceWorkerGlobalScope`, `Clients`, `Client`, `WindowClient`, `ExtendableEvent`, `FetchEvent`, `ExtendableMessageEvent`, `skipWaiting()`, `importScripts()`.
2a. **`MessageChannel`/`MessagePort`** (customer decision `D-3`, §17.3): full support, including transferring ports through `postMessage` in both directions and `ExtendableMessageEvent.ports`. This is required because the request/response pattern used by real-world service-worker libraries (`postMessage(msg, [channel.port2])` + `port1.onmessage`) fails silently without it, violating `R1.1.1`.
3. **Cache API**: `CacheStorage` (`caches`) and `Cache`, with all query options and batch semantics, available in both the client realm and the SW realm.
4. **All lifecycle algorithms** of the Spec: the job queue (`register`/`update`/`unregister`), soft update, install, activate, clear registration, worker/registration state updates, controller change notification, termination and the "no pending events" rule.
5. **Fetch interception**: Handle Fetch, Handle Functional Event, `respondWith` response validation, navigation preload, client-id assignment.
6. **A Fetch-API subset** sufficient for the above: `Headers`, `Request`, `Response`, the Body mixin (`arrayBuffer`, `blob`→out of scope, `text`, `json`, `formData` optional), and a `fetch()` function inside SW realms, all backed by a host `HttpClient` trait.
7. **A DOM shim**: `EventTarget`, `Event`, `MessageEvent`, `ErrorEvent`, `DOMException`, `AbortController`/`AbortSignal`, with the full WHATWG dispatch algorithm — switchable off when the host already provides these.
8. **Persistence**: pluggable storage with an in-memory backend and a production SQLite backend, covering registrations, script resource maps and cache storage.
9. **Boa integration**: realm-per-worker execution, job/microtask pumping, extended-lifetime-promise accounting, cooperative termination, structured-clone messaging between realms.
10. **Host API**: a documented Rust surface for driving all of the above from the embedder, plus tracing/observability.

### 1.4. Out of scope (v1.0)

| Item | Decision / rationale |
|---|---|
| Push API, Notifications, Background Sync, Periodic Sync, Background Fetch, Payment Handler, Cookie Store | Separate specifications. The event-dispatch machinery MUST be extensible (§11.3) so these can be added in v1.1 without redesign; no interfaces are exposed in v1.0. |
| `ReadableStream` bodies, `Response.body`, streaming `respondWith` | Boa has no Streams implementation. Bodies are fully buffered `Vec<u8>` with a configurable size cap (`R8.2.6`). `Response.prototype.body` MUST be present and return `null`. |
| `Blob`, `File`, `FormData` beyond `application/x-www-form-urlencoded` | No File API in Boa. `Response.blob()` MUST reject with `NotSupportedError`; `formData()` supports only urlencoded bodies. |
| Content Security Policy enforcement, Trusted Types, COOP/COEP | Host responsibility; the host MAY reject script fetches through `HttpClient`. |
| Cross-process / multi-`Context` sharing of one storage key | v1.0 assumes one `Context` per storage key per process; a second process opening the same SQLite database is rejected with an advisory lock error (§13.6). |
| Real preemptive termination of a running worker | Cooperative only (§11.5): budget hook + host callback. A worker in an infinite loop cannot be killed without engine support; documented limitation. |
| `SharedWorker` clients | `Client.type` MUST support `"window"` and `"worker"`; `"sharedworker"` values from the host MUST be accepted and passed through but are never created by this crate. |
| Sub-resource integrity, `importScripts` from cross-origin URLs with opaque responses | Cross-origin imports are fetched through the host client; opaque responses MUST fail the update (`SecurityError`). |
| Automatic HTTP cache | The crate has no HTTP cache. `updateViaCache` is translated into a `CachePolicy` value passed to the host `HttpClient`, which decides (§6.4.4). |

Every exclusion MUST be repeated in `docs/compat.md` with the observable behaviour (which exception, which value).

### 1.5. Normative and reference documents

1. W3C **Service Workers** — https://www.w3.org/TR/service-workers/ — primary source of requirements; anchors such as `#handle-fetch` refer to it.
2. WHATWG **Fetch** — request/response concepts, response types, `Vary`, redirect handling, CORS terminology.
3. WHATWG **DOM** — `EventTarget`, `Event`, dispatch algorithm, `DOMException` names.
4. WHATWG **HTML** — `navigator.serviceWorker`, worker global scopes, structured serialization, event loop, task sources, message ports.
5. WHATWG **URL** — parsing, serialization, origin, path.
6. WHATWG **Infra** — lists, maps, byte sequences, ordered map iteration.
7. **Web IDL** — argument conversion, `EnforceRange`, `[SameObject]`, brand checks, exception names.
8. ECMA-262 — jobs, promises, realms, module semantics.
9. Boa 0.22 documentation and source: `core/engine/src/{context,job,realm,class}.rs`, `core/runtime/src/{fetch,url,text,message}`, `core/wintertc/src/{store,abort,clone,events}`. Verified findings are recorded in `docs/BOA_022_PLATFORM_NOTES.md` (delivered with this TS).
10. `boa-idb` TS (`TZ_boa_idb_IndexedDB.md`) and `docs/BOA_022_INTEGRATION_GUIDE.md` — reusable patterns for JS bindings, GC-safe native data, and WebIDL conversion. Reuse them; do not reinvent.
11. web-platform-tests, directory `service-workers/`.

---

## 2. Target platform, dependencies and project structure

### 2.1. Toolchain

| Parameter | Value |
|---|---|
| MSRV | **1.91.0** (matches `boa_engine` 0.22) |
| Edition | **2024** |
| `rust-toolchain.toml` | channel `1.91.0`, components `rustfmt`, `clippy` |
| CI platforms | `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` |
| `wasm32-unknown-unknown` | `boa_sw_core` MUST compile with and without default features; checked in CI. The target's `std` is available and **is** used — a true `no_std` core is impossible because `url` and `http` require `std`. What the wasm build excludes is the storage backends and the host clock implementations |
| Lints | `unsafe_code = "deny"`, `missing_docs = "warn"`, `clippy::pedantic = "warn"` at workspace level |

`R2.1.1` — `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings` and `cargo deny check` MUST be clean at the end of every task.
`R2.1.2` — no `unwrap()`/`expect()`/`panic!()` in library code outside tests; use `thiserror` error types. Documented-infallible cases require a comment naming the invariant.
`R2.1.3` — no blocking I/O and no thread spawning inside `boa_sw_core`.

### 2.2. Workspace layout

```
boa-sw/
├── Cargo.toml                  # workspace, shared lints and dependency versions
├── rust-toolchain.toml
├── clippy.toml
├── deny.toml
├── AGENTS.md                   # working contract for implementers (delivered)
├── TZ_boa_sw_ServiceWorkers.md # this document
├── docs/
│   ├── SPEC_REVISION.md        # pinned Spec revision + editor's draft SHA
│   ├── BOA_022_PLATFORM_NOTES.md
│   ├── DECISIONS.md            # ADR log; one entry per decision/new dependency
│   ├── QUESTIONS.md            # escalations
│   ├── compat.md               # deviations from the Spec (see §1.4)
│   ├── traceability.md         # Spec algorithm → module → test
│   └── reviews/T-<NN>-handoff.md
├── crates/
│   ├── boa_sw_core/            # L1: pure Rust model + algorithms, no boa deps
│   ├── boa_sw_fetch/           # L2: fetch data model + JS classes (needs boa)
│   ├── boa_sw_dom/             # L2: EventTarget/Event/DOMException shim
│   ├── boa_sw/                 # L3: JS bindings, realms, runtime, host API
│   ├── boa_sw_memory/          # storage backend: in-memory
│   ├── boa_sw_sqlite/          # storage backend: SQLite
│   └── boa_sw_wpt/             # WPT harness (bin + lib), not published
├── examples/
│   └── offline_shell/          # end-to-end demo host (§18 Appendix D)
└── fuzz/                       # cargo-fuzz targets
```

`R2.2.1` — dependency direction is strictly one-way:
`boa_sw_core` ← `boa_sw_fetch` ← `boa_sw` ; `boa_sw_dom` ← `boa_sw` ; `boa_sw_{memory,sqlite}` depend only on `boa_sw_core`; `boa_sw_wpt` depends on everything. A dependency edge in any other direction is a defect.
`R2.2.2` — `boa_sw_core` MUST NOT list `boa_engine`, `boa_gc`, `boa_runtime` or `boa_wintertc` in any dependency section, including dev-dependencies.

### 2.3. Dependencies

Allowed without further approval (workspace-pinned):

| Crate | Version | Where | Purpose |
|---|---|---|---|
| `boa_engine`, `boa_gc`, `boa_macros` | `~0.22.0` | L2, L3 | engine |
| `boa_runtime`, `boa_wintertc` | `~0.22.0` | L3 (optional features) | `JsValueStore` structured clone, `URL`, `TextEncoder/Decoder`, `AbortSignal` |
| `thiserror` | 2.0 | all | error types |
| `url` | 2.5 | core, L2 | URL parsing/serialization (WHATWG) |
| `http` | 1.x | core, L2 | `Method`, `StatusCode`, `HeaderMap`, `Uri` |
| `indexmap` | 2.7 | core | ordered maps (headers, cache entries, job queues) |
| `smallvec` | 1.13 | core | small vectors |
| `hashbrown` | 0.15 | core | maps |
| `bitflags` | 2 | core | flag sets |
| `tracing` | 0.1 | all (feature `tracing`) | observability |
| `rusqlite` | 0.32 (bundled) | `boa_sw_sqlite` | SQLite backend |
| `sha2` | 0.10 | core | script/response digests for byte-for-byte comparison and cache keys |
| `proptest`, `rstest`, `insta` | current | dev | property, parametrised and snapshot tests |
| `futures-lite`, `futures-channel` | current | L3 | async plumbing for `NativeAsyncJob` |

`R2.3.1` — any dependency outside this table requires an ADR entry in `docs/DECISIONS.md` (maintained? widely used? permissive licence?) **before** first use.
`R2.3.2` — `boa_sw_core` MUST be usable with `default-features = false` and only `thiserror`, `url`, `http`, `indexmap`, `smallvec`, `hashbrown`, `bitflags`, `sha2`.

### 2.4. Cargo features

`boa_sw_core`:

| Feature | Default | Effect |
|---|---|---|
| `std` | yes | std-only conveniences (a `SystemClock` implementation, `std::error::Error` impls beyond `thiserror`'s, filesystem-free debug dumps). The crate always links `std` through `url`/`http`; disabling this feature removes the conveniences, not the `std` dependency |
| `tracing` | no | emit `tracing` spans/events |
| `serde` | no | `Serialize`/`Deserialize` for persistent records (used by backends and by snapshot tests) |
| `test-util` | no | deterministic fakes shipped for downstream crates and tests: `FakeClock`, `FailingStorage`, the storage contract suite (§4.2.4, T-08) |

`boa_sw`:

| Feature | Default | Effect |
|---|---|---|
| `dom-shim` | yes | register `EventTarget`, `Event`, `MessageEvent`, `ErrorEvent`, `DOMException`, `AbortController` from `boa_sw_dom`. When off, the host MUST have registered compatible classes and MUST supply a `DomBridge` (§4.2.7) |
| `fetch-classes` | yes | register `Headers`, `Request`, `Response`, `fetch` from `boa_sw_fetch` |
| `memory-backend` | yes | re-export `boa_sw_memory` and allow `SwConfig::memory()`; used by tests and by hosts that do not need persistence |
| `sqlite-backend` | **yes** | re-export `boa_sw_sqlite`. **This is the default and recommended backend** (customer decision `D-1`, §17.3): documentation, `examples/`, the demo host and the WPT run all use it. Hosts that must avoid the bundled SQLite C build (wasm, minimal builds) use `--no-default-features --features memory-backend` |
| `module-workers` | yes | support `{ type: "module" }` registrations (§11.4.3) |
| `navigation-preload` | yes | `NavigationPreloadManager` and `FetchEvent.preloadResponse` |
| `runtime-interop` | no | adapters to `boa_runtime`: `Fetcher` → `HttpClient`, reuse of `boa_runtime::url::Url`, `structuredClone` via `boa_wintertc` |
| `tracing` | no | forwards to `boa_sw_core/tracing` |

`R2.4.1` — every feature combination in the CI matrix (§15.6) MUST compile and pass tests; feature-gated code MUST NOT change observable behaviour of unrelated features.
`R2.4.2` — the default build MUST produce a persistent runtime: `README.md`, every example, `docs/host-integration.md` and the WPT harness default to `boa_sw_sqlite`. A host that selects the memory backend MUST be warned once through `SwObserver` at `install` time that registrations and caches will not survive the process.

### 2.5. Verified Boa 0.22 platform surface

The following was verified against the Boa checkout used by this project (see `docs/BOA_022_PLATFORM_NOTES.md` for details, file paths and code snippets). Implementers MUST rely on these facts rather than assuming a browser-like environment.

| Facility | Status in Boa 0.22 | Consequence for `boa-sw` |
|---|---|---|
| Multiple realms in one `Context` | **Available**: `Context::create_realm()`, `Context::enter_realm(realm) -> Realm`, `Realm::host_defined()`, per-realm class registration | Basis of `AD-1`: one realm per running service worker |
| Jobs with an explicit realm | **Available**: `NativeAsyncJob::with_realm(f, realm)`, `Job::{PromiseJob, AsyncJob, TimeoutJob, GenericJob}`, `JobExecutor` trait | Basis of `AD-4`/§11.2 scheduling |
| Structured clone | **Available**: `boa_wintertc::store::JsValueStore` — context-free, `Send`, supports transfer lists | Basis of `AD-6`; used for every cross-realm payload |
| `EventTarget`/`Event`/`MessageEvent` | **Missing** — `boa_wintertc::events::register()` is a stub returning `Ok(())` | `boa_sw_dom` MUST implement them (task `T-10`) |
| Fetch classes | **Partial**: `boa_runtime::fetch` provides `JsRequest`/`JsResponse`/`JsHeaders` over `http` types and a `Fetcher` trait, but bodies are buffered `Vec<u8>`, and `Response` lacks `type`/`redirected`/`url` construction control needed by the Spec | `boa_sw_fetch` owns its own classes (`AD-8`); `runtime-interop` provides adapters |
| `URL`, `TextEncoder`/`TextDecoder`, `structuredClone`, `atob`/`btoa`, timers, `AbortController` | **Available** in `boa_runtime`/`boa_wintertc` | Reuse; never re-implement. `boa_sw` MUST NOT register them itself (the host does) but MUST tolerate their absence |
| `JsString` UTF-16 fidelity | `to_vec() -> Vec<u16>`, `code_unit_at`, `JsString::from(&[u16])` | Use these; `to_std_string_escaped()` is forbidden anywhere data fidelity matters |
| Native data in objects | `JsObject::from_proto_and_data`, `downcast_ref::<T>() -> Option<GcRef<T>>`, `#[derive(Trace, Finalize, JsData)]`, `#[unsafe_ignore_trace]` | Object model per §3.3 |
| Class macros | `#[boa_class]`, `#[boa_module]`, `Class` trait, `TryFromJs`/`TryIntoJs` derives | Preferred way to declare JS classes |

---

## 3. Architecture

### 3.1. Layers

```
┌──────────────────────────────────────────────────────────────────────────┐
│ L4  Host application (BoaX browser, CLI, tests)                          │
│     owns: Context, network stack, windows/documents, disk                │
│     calls: SwRuntime::install / pump / handle_fetch / on_client_*        │
│     implements: HttpClient, ClientHost, Clock, SwStorage                 │
└──────────────▲───────────────────────────────────────────────────────────┘
               │ host API (§4)
┌──────────────┴───────────────────────────────────────────────────────────┐
│ L3  crate `boa_sw` — bindings + runtime                                  │
│     • SwRuntime: agents, realms, scheduler, ticket pumps                 │
│     • JS classes: container/registration/worker/global scope/clients/... │
│     • Cache & CacheStorage JS classes over core algorithms               │
│     • ELP (waitUntil/respondWith) accounting, script evaluation          │
└──────────────▲──────────────────────────▲────────────────────────────────┘
               │                          │
┌──────────────┴─────────────┐ ┌──────────┴────────────────────────────────┐
│ L2 `boa_sw_fetch`          │ │ L2 `boa_sw_dom`                           │
│  Headers/Request/Response  │ │  EventTarget/Event/MessageEvent/          │
│  Body mixin, fetch()       │ │  ErrorEvent/DOMException/AbortController  │
└──────────────▲─────────────┘ └───────────────────────────────────────────┘
               │
┌──────────────┴───────────────────────────────────────────────────────────┐
│ L1  crate `boa_sw_core` — pure Rust, no JS engine                        │
│     model (registrations, workers, scripts, clients), job queue,         │
│     lifecycle algorithms, scope matching, fetch decision logic,          │
│     cache query/batch algorithms, storage traits, error taxonomy         │
└──────────────▲───────────────────────────────────────────────────────────┘
               │ SwStorage
┌──────────────┴───────────────────────────────────────────────────────────┐
│ L0  storage backends: `boa_sw_memory`, `boa_sw_sqlite`                   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.2. Architectural decisions (mandatory)

These decisions are **fixed**. An implementer who believes one is wrong MUST stop and escalate through `docs/QUESTIONS.md`; they MUST NOT deviate silently.

**`AD-1` — Realm per service worker, one `Context`, one thread.**
Each *running* service worker instance owns a dedicated Boa realm created with `Context::create_realm()`. The realm's global object is turned into a `ServiceWorkerGlobalScope` (§5.2.1). Client JavaScript keeps running in the host's realm. All realms live in a single `Context` on a single OS thread; there is no parallelism between a worker and its clients. Rationale: `Context` is `!Send`; realms give per-worker globals and per-realm class registration at a fraction of the complexity of multi-context/multi-thread; `NativeAsyncJob::with_realm` already carries realm affinity. Every cross-realm interaction MUST nevertheless go through the message/ticket boundary defined in `AD-6`, so that a future thread-per-worker mode is a runtime change only, not an API change.

**`AD-2` — Engine-free core.**
All model state and all Spec algorithms that do not require executing JavaScript live in `boa_sw_core` and are expressed over plain Rust types. `boa_sw_core` never sees `JsValue`. Anything the algorithms need from JS (e.g. "did the install handler's promise reject?") is passed in as a plain Rust outcome value.

**`AD-3` — Everything environmental is injected.**
Network, clients, clock, storage, and random ids are traits supplied by the host (§4.2). Core code MUST NOT read the system clock, open sockets or touch the filesystem. Tests use deterministic fakes for all four.

**`AD-4` — Deterministic, non-async core; async only at the edges.**
Core algorithms are synchronous state machines: they consume *events* (`CoreEvent`) and produce *commands* (`CoreCommand`). Waiting on the network, on script evaluation or on a promise is expressed as "issue command, park the job, resume on the matching event". No `async fn`, no futures, no callbacks inside `boa_sw_core`. Rationale: a state machine is exhaustively testable without an engine and is trivially replayable.

**`AD-5` — Job queue per scope, strictly FIFO.**
Registration jobs (`register`, `update`, `unregister`) are queued per `(storage key, scope url)` and executed one at a time in insertion order, exactly as `#job-queue` requires, including job equivalence and promise coalescing.

**`AD-6` — Structured clone at every realm boundary.**
`postMessage` payloads, `ExtendableMessageEvent.data`, and any value crossing from a client realm to a SW realm (or back) MUST be serialized with `boa_wintertc::store::JsValueStore` (feature `runtime-interop`) or the crate's own equivalent when that feature is off. Passing a `JsObject` created in one realm into another realm is a defect.

**`AD-7` — Buffered bodies only.**
Request and response bodies are `Rc<Vec<u8>>`. There are no streams. A configurable `max_body_bytes` (default 32 MiB) causes `HttpClient` results larger than the cap to be turned into a network error, and `Response`/`Request` constructors to throw `RangeError`.

**`AD-8` — `boa-sw` owns the Fetch classes.**
`Headers`, `Request`, `Response` and `fetch` used by Service Workers come from `boa_sw_fetch`, because the Spec needs request `mode`/`credentials`/`destination`/`cache` and response `type`/`redirected`/`url`, which `boa_runtime::fetch` does not model. On registration, `boa_sw` MUST detect an existing global `Request`/`Response`/`Headers`/`fetch` in the target realm and fail with a clear error (`SwError::ConflictingGlobals`) unless the host explicitly opted in via `SwConfig::allow_global_overwrite` or disabled `fetch-classes`.

**`AD-9` — All time comes from `Clock`.**
Last-update-check timestamps, the 86 400 s soft-update rule, idle timeouts, and event timeouts read `Clock::now_ms()`/`monotonic_ms()`. Tests drive a `FakeClock`.

**`AD-10` — Cooperative termination.**
Terminating a worker means: reject its outstanding ELPs, drop its realm and pending jobs, mark the worker record as not running. Interrupting JavaScript that is currently on the stack is out of scope; the runtime exposes a budget hook the host may wire to Boa's interrupt facilities when available (§11.5).

**`AD-11` — One storage trait family, atomic batches.**
Registrations, script resource maps and caches are persisted through a single `SwStorage` trait (§4.2.4). A lifecycle transition that must be durable (install completed, activation completed, registration cleared) is written as one `StorageBatch` applied atomically. A crash between batches MUST leave a state that recovery (§7.11) can repair.

**`AD-12` — Explicit error taxonomy.**
`boa_sw_core::SwError` is the single internal error type; §12 gives the total mapping from `SwError` variants to JavaScript exceptions (`DOMException` name or native error). Bindings MUST NOT invent messages or names outside that table.

### 3.3. Object model and GC

`R3.3.1` — every JS class implemented by this workspace stores its state in a native struct with `#[derive(Trace, Finalize, JsData)]`, attached via `JsObject::from_proto_and_data`.
`R3.3.2` — native structs store **identifiers** (`RegistrationId`, `WorkerId`, `ClientId`, `CacheId`) and, where the Spec requires object identity, GC-traced `JsObject` handles. Raw pointers, `Rc<RefCell<…>>` graphs of JS objects, and `'static` references are forbidden.
`R3.3.3` — `#[unsafe_ignore_trace]` is allowed only on types that provably contain no GC-managed values (numeric ids, enums, `Url`, `http::HeaderMap`, `Vec<u8>`).
`R3.3.4` — object identity required by the Spec:
* `ServiceWorkerRegistration` objects are per-realm and per-registration: two calls that observe the same registration from the same realm MUST return the same JS object (`[SameObject]`-like behaviour required by `registration.installing === registration.installing`). Implemented with a per-realm `IdentityMap<RegistrationId, WeakJsObject>`; use `boa_gc::WeakGc` so entries do not leak.
* `ServiceWorker` objects likewise per `(realm, WorkerId)`.
* `Client` objects are **not** cached: each `clients.matchAll()` returns fresh objects (matches browsers and the Spec's "create a new Client object").
`R3.3.5` — a dropped realm MUST drop all identity maps belonging to it; `SwRuntime` MUST NOT hold strong references to realm-owned JS objects after the worker is terminated (verified by test `T-23` leak suite).

### 3.4. Concurrency, re-entrancy and borrow discipline

The single most common source of defects in this design is borrowing. The following rules are **normative** and MUST be repeated verbatim in `crates/boa_sw/src/runtime.rs` module docs.

`R3.4.1` — There is exactly one `&mut Context`, owned by the host and passed down. `SwRuntime` is stored as `Rc<SwRuntime>` in the *client realm's* `host_defined` and in `Context` data; all mutable state inside it is behind `RefCell`.
`R3.4.2` — **Never hold a `RefCell` borrow across a call into JavaScript.** Any of `JsObject::get/set`, `call`, `eval`, `JsPromise::then`, event dispatch, or a `NativeFunction` invocation may re-enter `SwRuntime`. The mandatory pattern is: borrow → copy out the data needed → drop the borrow → call JS → borrow again to record the result.
`R3.4.3` — Realm entry is stack-like and MUST use the helper
```rust
pub fn with_realm<R>(
    context: &mut Context,
    realm: &Realm,
    f: impl FnOnce(&mut Context) -> JsResult<R>,
) -> JsResult<R>
```
which does `let prev = context.enter_realm(realm.clone());`, runs `f`, and restores `prev` on both the `Ok` and the `Err` path. Direct calls to `Context::enter_realm` outside this helper are forbidden.
`R3.4.4` — `SwRuntime::pump` is non-reentrant: it sets an `in_pump` flag and returns `SwError::Reentrant` if called while set.
`R3.4.5` — Host callbacks that arrive from other threads (`FetchCompleter`, `HostTicket`) MUST only enqueue into an MPSC channel; they MUST NOT touch `Context` or `SwRuntime` state. The channel is drained at the start of `pump`.
`R3.4.6` — Ordering guarantee: within one `pump` call, work is processed in this order, each phase to exhaustion before the next: (1) drain host completions, (2) expire timers/timeouts, (3) run core state machine steps (job queue), (4) dispatch pending functional events, (5) run engine jobs (microtasks and promise jobs) until the engine queue is empty, (6) evaluate termination decisions (idle workers). Phase 5 may produce new work for phases 3–4; `pump` loops until a full cycle produces no work or the `max_pump_iterations` budget (default 1000) is exhausted, in which case it returns `PumpOutcome::Budget`.

### 3.5. Principal data flows

**(a) `navigator.serviceWorker.register(url, opts)`**
1. Binding validates arguments (WebIDL), resolves `scriptURL` and `scope` against the client's base URL, checks same-origin and secure-context, creates a JS promise, and hands `CoreEvent::ScheduleJob(Job::Register{…, promise: JobPromiseId})` to core.
2. Core appends the job to the scope's queue; if it is the head, it starts it and emits `CoreCommand::FetchScript{…}`.
3. `SwRuntime` calls `HttpClient::start`; on completion the host pushes `CoreEvent::ScriptFetched{…}`.
4. Core performs byte-for-byte comparison (§6.4). If unchanged: `CoreCommand::ResolveJobPromise(reg)`. If changed/new: `CoreCommand::EvaluateScript{worker, sources}`.
5. `SwRuntime` creates a SW realm, evaluates the script, reports `CoreEvent::ScriptEvaluated{ok|error}`.
6. Core runs Install: emits `CoreCommand::DispatchEvent{worker, install}`; bindings dispatch, collect ELPs, and report `CoreEvent::EventHandled{worker, event, outcome}`.
7. Core updates states, emits `CoreCommand::Persist(batch)` and `CoreCommand::ResolveJobPromise`, then either Activate or park in `waiting`.

**(b) `handle_fetch`** — §8.1. Host → `SwRuntime::handle_fetch` → core decides (match registration, active worker, should-skip-event) → either `Fallback` immediately, or start the worker, dispatch `FetchEvent`, wait for `respondWith`, validate the response, return it to the host.

**(c) `cache.put(req, res)`** — binding converts to core types → core validates (§9.3) → `SwStorage::cache_batch` applied atomically → promise resolved.

**(d) `client.postMessage(data)` from a worker** — binding serializes with `JsValueStore` → runtime routes: local client realm ⇒ enqueue into the client's message queue and deliver during `pump`; foreign client ⇒ `ClientHost::deliver_message`.

---

## 4. Host-side Rust API

All items in this section are public API of `boa_sw` unless stated otherwise. Signatures are **normative**: implementers MUST match names, argument order and types.

### 4.1. Installation into a `Context`

```rust
pub struct SwConfig {
    /// Storage key (isolation unit) of the client realm, e.g. "https://example.com".
    pub storage_key: StorageKey,
    /// Base URL of the client realm, used to resolve relative script/scope URLs.
    pub base_url: Url,
    /// Treat the origin as a secure context even when the scheme is not https (default false).
    pub allow_insecure_origin: bool,
    /// Allow overwriting pre-existing globals named Request/Response/Headers/fetch/caches.
    pub allow_global_overwrite: bool,
    /// Register `caches` in the client realm too (default true).
    pub caches_in_client_realm: bool,
    /// Limits and timeouts.
    pub limits: SwLimits,
}

pub struct SwLimits {
    pub max_body_bytes: u64,             // default 32 * 1024 * 1024
    pub max_script_bytes: u64,           // default 8 * 1024 * 1024
    pub max_imported_scripts: u32,       // default 100
    pub max_registrations_per_key: u32,  // default 1024
    pub cache_quota_bytes: Option<u64>,  // default None (host-enforced)
    pub install_event_timeout_ms: u64,   // default 300_000
    pub functional_event_timeout_ms: u64,// default 300_000
    pub worker_idle_timeout_ms: u64,     // default 30_000
    pub max_pump_iterations: u32,        // default 1000
}

pub struct SwHost {
    pub http: Rc<dyn HttpClient>,
    pub clients: Rc<dyn ClientHost>,
    pub clock: Rc<dyn Clock>,
    pub storage: Rc<dyn SwStorage>,
    pub observer: Option<Rc<dyn SwObserver>>,
}

impl SwRuntime {
    /// Creates the runtime, restores persisted registrations for `config.storage_key`,
    /// and registers `navigator.serviceWorker` (+ `caches`, + fetch/DOM classes per features)
    /// into the *current realm* of `context`, which becomes the client realm.
    pub fn install(config: SwConfig, host: SwHost, context: &mut Context)
        -> Result<Rc<SwRuntime>, SwError>;

    /// Retrieves the runtime previously installed into this context.
    pub fn get(context: &mut Context) -> Option<Rc<SwRuntime>>;
}
```

`R4.1.1` — `install` MUST be idempotent-safe: a second call on the same `Context` returns `SwError::AlreadyInstalled`.
`R4.1.2` — `install` MUST load persisted registrations (`SwStorage::load_registrations`) and reconstruct the registry, marking every worker as *not running* (workers are started lazily), and repairing inconsistent states per §7.11.
`R4.1.3` — `install` MUST NOT start any worker and MUST NOT perform I/O beyond the single `load_registrations` call.

### 4.2. Host traits

#### 4.2.1. `Clock`

```rust
pub trait Clock: 'static {
    /// Wall-clock time, milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
    /// Monotonic milliseconds; MUST never decrease.
    fn monotonic_ms(&self) -> u64;
}
```

#### 4.2.2. `HttpClient`

```rust
pub struct FetchTicketId(pub u64);

pub struct FetchCompleter { /* Send; holds an mpsc sender + ticket id */ }
impl FetchCompleter {
    pub fn complete(self, result: Result<SwResponse, NetworkError>);
}

pub trait HttpClient: 'static {
    /// Start a network fetch. The implementation MUST eventually call
    /// `completer.complete(...)` exactly once, from any thread.
    fn start(&self, id: FetchTicketId, request: SwRequest, completer: FetchCompleter);

    /// Best-effort cancellation. After this call the runtime ignores a late completion.
    fn cancel(&self, id: FetchTicketId);
}
```

`R4.2.1` — `SwRequest` carries everything the Spec's fetch needs: method, url, headers, body, `mode`, `credentials`, `cache` policy, `redirect` policy, `destination`, `referrer`, `is_reload`, `client_id`, `keepalive`. See §8.
`R4.2.2` — script fetches MUST set `destination = ServiceWorker` (or `Script` for imports), MUST add the header `Service-Worker: script`, and MUST use `redirect = Error`.
`R4.2.3` — the runtime MUST tolerate a completion arriving after `cancel` or after the associated worker was terminated (drop it, emit an observer event).

#### 4.2.3. `ClientHost`

```rust
pub struct ClientRecord {
    pub id: ClientId,                 // opaque, host-assigned, stable for the client's lifetime
    pub url: Url,
    pub kind: ClientKind,             // Window | Worker | SharedWorker
    pub frame_type: FrameType,        // Auxiliary | TopLevel | Nested | None
    pub storage_key: StorageKey,
    pub visibility: VisibilityState,  // Hidden | Visible  (windows only)
    pub focused: bool,                // windows only
    pub focus_order: u64,             // higher = more recently focused; used by matchAll ordering
    pub ancestor_origins: Vec<String>,
    pub is_local: bool,               // true when the client runs in the runtime's client realm
    pub execution_ready: bool,
}

pub trait ClientHost: 'static {
    fn get(&self, id: &ClientId) -> Option<ClientRecord>;
    fn list(&self, storage_key: &StorageKey) -> Vec<ClientRecord>;

    /// Asynchronous window operations; the host completes the ticket exactly once.
    fn focus(&self, id: &ClientId, ticket: HostTicket<Option<ClientRecord>>);
    fn navigate(&self, id: &ClientId, url: Url, ticket: HostTicket<Result<Option<ClientRecord>, NavigateError>>);
    fn open_window(&self, url: Url, ticket: HostTicket<Result<Option<ClientRecord>, NavigateError>>);

    /// Deliver a message to a non-local client. Local clients are handled by the runtime.
    fn deliver_message(&self, id: &ClientId, message: ClientMessage) -> Result<(), SwError>;

    /// Called by the runtime when the set of controlled clients changes, so the host can
    /// route future navigations/subresource loads through `handle_fetch`.
    fn on_controller_changed(&self, id: &ClientId, controller: Option<WorkerId>);
}
```

`R4.2.4` — `ClientHost::list` MUST return clients of *all* storage keys the host knows for that key's origin; the runtime filters. Ordering requirements of `clients.matchAll()` (§5.2.3) are applied by the runtime, not the host.
`R4.2.5` — `ClientMessage` carries a `JsValueStore`-serialized payload (or the crate's serialized form), the source worker id, the source origin and transferred ports; it is `Send`.

#### 4.2.4. `SwStorage`

```rust
pub trait SwStorage: 'static {
    // --- registrations -------------------------------------------------
    fn load_registrations(&self, key: &StorageKey) -> StorageResult<Vec<PersistedRegistration>>;
    fn apply(&self, key: &StorageKey, batch: StorageBatch) -> StorageResult<()>;
    fn load_script_map(&self, worker: WorkerId) -> StorageResult<ScriptResourceMap>;

    // --- cache storage -------------------------------------------------
    fn cache_list(&self, key: &StorageKey) -> StorageResult<Vec<CacheName>>;
    fn cache_open(&self, key: &StorageKey, name: &CacheName, create: bool)
        -> StorageResult<Option<CacheId>>;
    fn cache_delete(&self, key: &StorageKey, name: &CacheName) -> StorageResult<bool>;
    fn cache_query(&self, cache: CacheId, query: &CacheQuery) -> StorageResult<Vec<CacheEntryId>>;
    fn cache_read(&self, cache: CacheId, entry: CacheEntryId) -> StorageResult<CacheEntry>;
    fn cache_keys(&self, cache: CacheId) -> StorageResult<Vec<CacheEntryId>>;
    fn cache_batch(&self, cache: CacheId, ops: Vec<CacheOperation>) -> StorageResult<CacheBatchReport>;

    // --- quota ---------------------------------------------------------
    fn usage(&self, key: &StorageKey) -> StorageResult<u64>;
}
```

`R4.2.6` — `apply` and `cache_batch` MUST be atomic: either all operations are durable, or none are.
`R4.2.7` — `cache_batch` MUST validate the whole operation list before mutating anything, and report `QuotaExceeded` without partial writes.
`R4.2.8` — every method MUST be safe to call re-entrantly from within a `pump` (backends must not call back into the runtime).

#### 4.2.5. `SwObserver` (optional)

```rust
pub trait SwObserver: 'static {
    fn on_event(&self, ev: &ObserverEvent);   // job started/finished, worker state change,
                                              // event dispatched, fetch intercepted, storage batch
}
```
`R4.2.9` — observer calls MUST NOT be able to affect behaviour; they are for logging, metrics and tests. When the `tracing` feature is on, the same information is emitted as `tracing` events.

#### 4.2.6. `HostTicket<T>`

```rust
pub struct HostTicket<T> { /* Send */ }
impl<T> HostTicket<T> {
    pub fn complete(self, value: T);
    pub fn id(&self) -> HostTicketId;
}
```
`R4.2.10` — completing a ticket twice MUST be a no-op for the second call (debug builds MAY assert). Dropping a ticket without completing it MUST be observable: the runtime rejects the associated promise with `AbortError` when the ticket's sender is dropped.

#### 4.2.7. `DomBridge` (only when `dom-shim` is off)

```rust
pub trait DomBridge: 'static {
    fn event_target_prototype(&self, ctx: &mut Context) -> JsResult<JsObject>;
    fn create_event(&self, ty: &str, init: EventInit, ctx: &mut Context) -> JsResult<JsObject>;
    fn dispatch(&self, target: &JsObject, event: &JsObject, ctx: &mut Context) -> JsResult<bool>;
    fn create_dom_exception(&self, name: &str, message: &str, ctx: &mut Context) -> JsResult<JsValue>;
}
```

### 4.3. Driving the runtime

```rust
impl SwRuntime {
    pub fn pump(&self, context: &mut Context) -> Result<PumpOutcome, SwError>;

    /// Ask whether a request must be intercepted, and start interception if so.
    pub fn handle_fetch(&self, request: SwRequest, opts: HandleFetchOptions) -> FetchInterception;

    pub fn on_client_created(&self, record: ClientRecord);
    pub fn on_client_execution_ready(&self, id: &ClientId);
    pub fn on_client_navigated(&self, id: &ClientId, url: Url);
    pub fn on_client_unload(&self, id: &ClientId);
    pub fn on_client_focus_changed(&self, id: &ClientId, focused: bool, focus_order: u64);
    pub fn on_client_visibility_changed(&self, id: &ClientId, v: VisibilityState);

    /// Delivers a message that the host routed to a local client (see ClientHost::deliver_message).
    pub fn deliver_message_to_local_client(&self, id: &ClientId, msg: ClientMessage);

    /// Graceful shutdown: `Handle User Agent Shutdown` (#on-user-agent-shutdown).
    pub fn shutdown(&self, context: &mut Context) -> Result<(), SwError>;

    // Introspection for hosts and tests.
    pub fn registrations(&self) -> Vec<RegistrationSnapshot>;
    pub fn controller_of(&self, client: &ClientId) -> Option<WorkerSnapshot>;
    pub fn is_idle(&self) -> bool;
}

pub enum PumpOutcome {
    /// No work left; `next_deadline_ms` tells the host when to call `pump` again (timers).
    Idle { next_deadline_ms: Option<u64> },
    /// Work remains (budget exhausted); call `pump` again as soon as possible.
    Pending,
    Budget,
}

pub struct FetchInterception { /* handle */ }
impl FetchInterception {
    pub fn state(&self) -> FetchInterceptionState;
    pub fn cancel(self);
}

pub enum FetchInterceptionState {
    /// The request is not controlled — the host performs the network fetch itself.
    NotIntercepted,
    /// Interception in progress; keep pumping.
    Pending,
    /// The worker produced this response; use it as the result of the fetch.
    Responded(SwResponse),
    /// The worker did not call respondWith; the host performs the network fetch itself.
    FallbackToNetwork,
    /// The worker failed; per the Spec this is a network error for the page.
    Failed(NetworkError),
}
```

`R4.3.1` — `handle_fetch` MUST return synchronously with `NotIntercepted` when core decides no active worker matches (`#handle-fetch` steps up to "If activeWorker is null"), so that uncontrolled navigations cost nothing.
`R4.3.2` — the host is expected to call `pump` until the interception state leaves `Pending`. `SwRuntime` MUST enforce `functional_event_timeout_ms`, after which the state becomes `Failed(NetworkError::Timeout)` and the worker's ELPs are rejected with `AbortError`.
`R4.3.3` — `shutdown` MUST: reject all outstanding job promises with `AbortError`, terminate every running worker (`AD-10`), flush any pending `StorageBatch`, and leave the registry in a state that `install` can restore identically.

---

## 5. JavaScript API surface

Appendix A contains the complete WebIDL that defines the implementation scope. This section states the behavioural requirements that the IDL does not capture.

**General binding rules (apply to every interface below):**

`R5.0.1` — every method and accessor MUST perform a **brand check** on `this` (native data of the expected type present) and throw `TypeError` otherwise.
`R5.0.2` — argument conversion follows Web IDL exactly: `DOMString` via `ToString`, dictionaries by member lookup in IDL order, `EnforceRange` where annotated, missing optional arguments take IDL defaults. Conversions that throw MUST do so **before** any side effect.
`R5.0.3` — interfaces without an IDL `constructor` MUST throw `TypeError` when invoked with `new` (`ServiceWorker`, `ServiceWorkerRegistration`, `ServiceWorkerContainer`, `Client`, `WindowClient`, `Clients`, `Cache`, `CacheStorage`, `NavigationPreloadManager`, `ServiceWorkerGlobalScope`).
`R5.0.4` — every interface MUST have `Symbol.toStringTag` equal to its interface name, and its prototype methods MUST be `writable: true, enumerable: true, configurable: true`; accessors `enumerable: true, configurable: true`; the constructor property `writable: true, enumerable: false, configurable: true`.
`R5.0.5` — all methods that the IDL declares as returning `Promise` MUST NEVER throw synchronously; argument-conversion failures and brand failures are returned as a rejected promise, except where the IDL says otherwise (`respondWith`, `waitUntil`, and the `EventTarget` methods throw synchronously).
`R5.0.6` — event handler IDL attributes (`onX`) follow the HTML "event handler attribute" semantics: setting a non-callable value clears the handler; the handler participates in dispatch in the position where it was *first* set.

### 5.1. Client context

#### 5.1.1. `ServiceWorkerContainer` — `navigator.serviceWorker`

`R5.1.1` — installed on the client realm's `navigator` object as a `[SameObject]` accessor. If `navigator` does not exist, `boa_sw` MUST create a minimal `Navigator` object (with `Symbol.toStringTag = "Navigator"`) — controlled by `SwConfig`.
`R5.1.2` — `register(scriptURL, options)`:
1. Reject with `TypeError` if the client's origin is opaque, or (unless `allow_insecure_origin`) not potentially trustworthy.
2. Parse `scriptURL` against the client base URL; on failure reject with `TypeError`.
3. If the parsed script URL's scheme is not `http`/`https` → `TypeError`; if its origin ≠ client origin → `SecurityError`.
4. If the script path contains `%2f` or `%5c` (case-insensitive) → `TypeError`.
5. Resolve `options.scope` (default: script URL with the last path segment replaced by `""`), same checks as 3–4 with `SecurityError`/`TypeError`.
6. Schedule a `register` job (§7.2) and return its promise.
`R5.1.3` — `getRegistration(clientURL)` resolves with the registration whose scope matches (`#scope-match-algorithm`) or `undefined`; a cross-origin `clientURL` rejects with `SecurityError`.
`R5.1.4` — `getRegistrations()` resolves with an array of every registration of the client's storage key whose scope's origin equals the client's origin, in registry insertion order.
`R5.1.5` — `ready` is a `[SameObject]`-like lazily created promise that resolves with the registration that has an **active** worker matching the client URL; if none exists yet, the promise stays pending and resolves when one appears (`#navigator-service-worker-ready`).
`R5.1.6` — `controller` returns the `ServiceWorker` object of the client's active worker or `null`; it MUST NOT change value while the client's JS is running, other than through the "Notify Controller Change" algorithm (§7.7.3), which fires `controllerchange`.
`R5.1.7` — messages sent to a client are queued and NOT delivered until either an `onmessage` handler is set, an `addEventListener("message", …)` call happens, or `startMessages()` is called (HTML's "port message queue" semantics). Requirement `R10.1.3` details the queue.

#### 5.1.2. `ServiceWorkerRegistration`

`R5.1.8` — `installing`/`waiting`/`active` return the per-realm `ServiceWorker` object for the corresponding worker or `null`, always reflecting the current core state at the moment of the getter call.
`R5.1.9` — `scope` returns the serialized scope URL; `updateViaCache` returns the stored mode.
`R5.1.10` — `update()` schedules an `update` job (§7.3) and rejects with `InvalidStateError` if the registration is unregistered, or `TypeError`/`SecurityError` per §7.3 failure modes.
`R5.1.11` — `unregister()` schedules an `unregister` job (§7.6); resolves `true` if a registration was removed, `false` otherwise.
`R5.1.12` — `onupdatefound` fires on the registration object in every realm that holds one, when a new `installing` worker is set (§7.7.2).
`R5.1.13` — `navigationPreload` returns a `[SameObject]` `NavigationPreloadManager` (feature `navigation-preload`).

#### 5.1.3. `ServiceWorker`

`R5.1.14` — extends `EventTarget`. `scriptURL` is the serialized script URL; `state` is one of `parsed`, `installing`, `installed`, `activating`, `activated`, `redundant`.
`R5.1.15` — `postMessage(message, transfer | options)`: serialize with structured clone; if the worker's state is `redundant` throw `InvalidStateError`; otherwise start the worker if needed and queue an `ExtendableMessageEvent` (§10.2). A `DataCloneError` from serialization is thrown **synchronously**.
`R5.1.16` — `statechange` is fired on every `ServiceWorker` object representing that worker, in every realm, whenever the worker's state changes (§7.7.1).

#### 5.1.4. `NavigationPreloadManager`

`R5.1.17` — `enable()`/`disable()` set the registration's navigation preload flag and persist it; both reject with `InvalidStateError` when the registration has no active worker.
`R5.1.18` — `setHeaderValue(value)` stores the value (default `"true"`), rejecting with `InvalidStateError` when there is no active worker.
`R5.1.19` — `getState()` resolves with `{ enabled, headerValue }`.

### 5.2. Worker context

#### 5.2.1. `ServiceWorkerGlobalScope`

`R5.2.1` — The SW realm's global object MUST: (a) have its prototype set to `ServiceWorkerGlobalScope.prototype`, whose prototype chain is `ServiceWorkerGlobalScope → WorkerGlobalScope → EventTarget → Object.prototype`; (b) expose `self` as a `[Replaceable]`-like own data property pointing at the global; (c) expose `Symbol.toStringTag = "ServiceWorkerGlobalScope"`.
`R5.2.2` — the SW realm global surface, in addition to the ECMAScript intrinsics, MUST contain exactly: `self`, `clients`, `registration`, `serviceWorker`, `caches`, `skipWaiting`, `importScripts` (classic scripts only), `fetch`, `Request`, `Response`, `Headers`, `Cache`, `CacheStorage`, `Client`, `WindowClient`, `Clients`, `ServiceWorker`, `ServiceWorkerRegistration`, `ServiceWorkerGlobalScope`, `ExtendableEvent`, `FetchEvent`, `ExtendableMessageEvent`, `Event`, `EventTarget`, `MessageEvent`, `ErrorEvent`, `DOMException`, `AbortController`, `AbortSignal`, `MessageChannel`, `MessagePort`, the `onX` handlers (`oninstall`, `onactivate`, `onfetch`, `onmessage`, `onmessageerror`, `onerror`), plus whatever the host injected through `SwConfig::extra_globals` (a list of `(JsString, JsValue-producing closure)`).
`R5.2.3` — `registration` and `serviceWorker` are `[SameObject]` per realm.
`R5.2.4` — `skipWaiting()` returns a promise; it sets the worker's skip-waiting flag and invokes "Try Activate" for its registration (§7.8.1); the promise resolves once activation has been attempted (not necessarily succeeded).
`R5.2.5` — an uncaught exception during initial script evaluation makes the worker fail (`#run-service-worker` step "If the script was not evaluated successfully") and the pending job rejects.
`R5.2.5b` — `importScripts(...urls)` is available only for classic workers; in a module worker it MUST throw `TypeError`. Semantics per §11.4.2.

#### 5.2.2. `ExtendableEvent`, `FetchEvent`, `ExtendableMessageEvent`

`R5.2.6` — `ExtendableEvent.waitUntil(p)`: throws `InvalidStateError` if the event is not active (dispatch has finished and no pending ELP keeps it alive); otherwise adds `p` to the event's ELP list and increments the worker's pending-events counter (§11.3).
`R5.2.7` — `FetchEvent.respondWith(r)`: throws `InvalidStateError` if the event is not active or `respond-with-entered` is already set; sets `respond-with-entered` and `wait-to-respond`; behaves as `waitUntil` with respect to lifetime. Resolution rules in §8.2.
`R5.2.8` — `FetchEvent` fields: `request` (`[SameObject]`), `preloadResponse` (promise; resolves `undefined` when preload is disabled), `clientId`, `resultingClientId`, `replacesClientId` (empty strings when not applicable), `handled` (promise resolved when the event's response has been determined, rejected on failure).
`R5.2.9` — `ExtendableMessageEvent` fields: `data`, `origin`, `lastEventId`, `source` (`Client`, `ServiceWorker`, or `MessagePort`; `null` when the source is gone), `ports`.

#### 5.2.3. `Clients`, `Client`, `WindowClient`

`R5.2.10` — `clients.get(id)` resolves with a `Client` for a client of the same origin whose id matches and which is "execution ready", else `undefined`.
`R5.2.11` — `clients.matchAll(options)`:
* default `type = "window"`, `includeUncontrolled = false`;
* filter: same origin as the worker; when `includeUncontrolled` is false, only clients whose active controller is this worker; type filter per `ClientQueryOptions.type` (`window`/`worker`/`sharedworker`/`all`);
* ordering: window clients first, ordered by most-recently-focused (descending `focus_order`), then creation order; non-window clients after them in creation order. This ordering MUST be produced by the runtime, and MUST be covered by a deterministic test.
`R5.2.12` — `clients.openWindow(url)` requires `url` to be same-origin-or-any http(s) URL; resolves with a `WindowClient` or `null` (host may refuse); rejects with `TypeError` for invalid URLs and with `InvalidAccessError` when the host reports the operation is not allowed.
`R5.2.13` — `clients.claim()` resolves `undefined`; when the worker is not active it rejects with `InvalidStateError`; otherwise it sets this worker as controller for every matching client that has no controller or a different one, firing `controllerchange` (§7.8.2).
`R5.2.14` — `Client` exposes `id`, `url`, `type`, `frameType`, `postMessage`; `WindowClient` adds `visibilityState`, `focused`, `ancestorOrigins`, `focus()`, `navigate(url)`.
`R5.2.15` — `WindowClient.navigate(url)` rejects with `TypeError` for `about:blank`-like URLs or cross-origin URLs, and with `InvalidAccessError` when the client is not controlled by this worker.

### 5.3. Cache API

`R5.3.1` — `caches` (a `CacheStorage`) is exposed in the SW realm and, per the Spec's `WindowOrWorkerGlobalScope` mixin, **also in the client realm by default** (`SwConfig::caches_in_client_realm = true`; customer decision `D-2`, §17.3). Both realms share the same storage key and the same backing store, and a page may `open`/`match`/`put`/`delete` without a worker running. Setting the flag to `false` is a documented deviation the host opts into (it MUST then be listed in `docs/compat.md` by that host, not by this crate).
`R5.3.2` — `CacheStorage`: `open(name)`, `has(name)`, `delete(name)`, `keys()`, `match(request, options)` where `options.cacheName` restricts the search; `keys()` returns names in creation order.
`R5.3.3` — `Cache`: `match`, `matchAll`, `add`, `addAll`, `put`, `delete`, `keys`, all promise-returning; semantics and error conditions in §9.
`R5.3.4` — `Cache.keys()` and `matchAll()` return **new** `Request`/`Response` objects each call; returned `Response` objects MUST have unused bodies (each call yields a fresh body copy).

### 5.4. Fetch subset (`boa_sw_fetch`)

`R5.4.1` — `Headers` implements the WHATWG guard model with guards `none`, `request`, `request-no-cors`, `response`, `immutable`; forbidden header names/response header names MUST be enforced; `append`/`set`/`delete`/`get`/`has`/`getSetCookie`, iteration (`entries`, `keys`, `values`, `Symbol.iterator`, `forEach`) with sorted-and-combined ordering per the Fetch spec.
`R5.4.2` — `Request(input, init)` supports `method`, `headers`, `body`, `mode` (`same-origin`, `no-cors`, `cors`, `navigate` — `navigate` only internally), `credentials`, `cache`, `redirect`, `referrer`, `referrerPolicy`, `integrity` (stored, not enforced), `keepalive`, `signal`, `destination` (read-only, set internally). `clone()` MUST throw `TypeError` when the body is disturbed.
`R5.4.3` — `Response(body, init)` supports `status` (200 default; `RangeError` outside 200–599), `statusText`, `headers`; static `Response.error()`, `Response.redirect(url, status)`, `Response.json(data, init)`; read-only `type`, `url`, `redirected`, `ok`, `status`, `statusText`, `headers`, `body` (always `null`, §1.4), `bodyUsed`; `clone()` (throws `TypeError` when disturbed).
`R5.4.4` — Body mixin methods `arrayBuffer()`, `text()`, `json()`, `bytes()`, `formData()` (urlencoded only) MUST set `bodyUsed` and reject a second consumption with `TypeError`.
`R5.4.5` — response type semantics MUST be modelled: `basic`, `cors`, `default`, `error`, `opaque`, `opaqueredirect`. An `opaque` response exposes status 0, empty headers, empty body, `url` = "".
`R5.4.6` — `fetch()` inside a SW realm MUST NOT be intercepted by the same or any service worker (no recursion); it goes straight to `HttpClient`.
`R5.4.7` — `fetch()` MUST honour `AbortSignal` when `runtime-interop` or the crate's own `AbortController` is present: aborting rejects with `AbortError` and calls `HttpClient::cancel`.

### 5.5. DOM shim (`boa_sw_dom`, feature `dom-shim`)

`R5.5.1` — `EventTarget`: `addEventListener(type, callback, options)` with `capture`, `once`, `passive`, `signal`; `removeEventListener`; `dispatchEvent`. Listener list semantics (duplicate detection by `(type, callback, capture)`), removal during dispatch, and `once` removal MUST follow the DOM Standard.
`R5.5.2` — `Event`: `type`, `target`, `currentTarget`, `eventPhase`, `bubbles`, `cancelable`, `defaultPrevented`, `composed`, `isTrusted`, `timeStamp`, `stopPropagation()`, `stopImmediatePropagation()`, `preventDefault()`, `composedPath()`.
`R5.5.3` — the dispatch algorithm is the flat (no-tree) version: capturing phase is skipped, `AT_TARGET` only, listeners invoked in registration order, `stopImmediatePropagation` honoured; `dispatchEvent` returns `!canceled`.
`R5.5.4` — `DOMException` with the full legacy name/code table, `Symbol.toStringTag`, `instanceof Error`, `stack` when the engine supplies one.
`R5.5.5` — `MessageEvent` and `ErrorEvent` per HTML; `AbortController`/`AbortSignal` per DOM (`abort()`, `reason`, `throwIfAborted()`, `AbortSignal.abort()`, `AbortSignal.timeout()` optional).
`R5.5.7` — `MessageChannel` and `MessagePort` per HTML: `new MessageChannel()` yields two entangled ports; `port.postMessage(message, transfer | options)`, `port.start()`, `port.close()`, `port.onmessage`, `port.onmessageerror`, and `MessagePort` as an `EventTarget`. A port's message queue starts disabled and is enabled by `start()` or by setting `onmessage` (setting `onmessage` implicitly starts the port; `addEventListener("message", …)` does **not**). Ports are registered in both the client realm and every SW realm. **Implementation note:** unlike the other classes of §5.5, `MessageChannel`/`MessagePort` live in `boa_sw::messaging` (task `T-27`), not in `boa_sw_dom`, because port entanglement and delivery need the runtime's pump; `boa_sw_dom` only supplies their `EventTarget` base and `MessageEvent`.
`R5.5.6` — when `dom-shim` is disabled, `boa_sw` MUST use `DomBridge` (§4.2.7) and MUST NOT register any of these globals.

### 5.6. Realm surfaces and conflicts

`R5.6.1` — client realm receives: `navigator.serviceWorker`, `caches` (optional), and, when the corresponding features are on and the globals are absent, the fetch and DOM classes.
`R5.6.2` — SW realm receives the full surface of `R5.2.2`; it MUST NOT receive `navigator.serviceWorker`, `window`, `document` or `localStorage`.
`R5.6.3` — a name collision on a global in either realm is an error at `install`/realm-creation time (`SwError::ConflictingGlobals { name }`) unless `allow_global_overwrite` is set, in which case the existing binding is replaced and a warning is emitted through `SwObserver`.

---

## 6. Core model: identifiers, URLs, registrations, scripts

### 6.1. Identifiers and storage key

```rust
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct StorageKey(String);          // host-supplied; normally the serialized origin
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] pub struct RegistrationId(u64);
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] pub struct WorkerId(u64);
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] pub struct JobId(u64);
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] pub struct DispatchId(u64);
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)] pub struct CacheId(u64);
#[derive(Clone, PartialEq, Eq, Hash, Debug)] pub struct ClientId(String);
```

`R6.1.1` — ids are allocated by a monotonically increasing counter held by `SwCore`; they are stable for the process lifetime. `WorkerId` and `RegistrationId` are **also persisted**, so a restored registry continues the counters above the maximum persisted value.
`R6.1.2` — `ClientId` values are opaque host strings; the runtime MUST NOT parse them, MUST NOT generate them, and MUST expose them verbatim in `Client.id`, `FetchEvent.clientId`, `resultingClientId`, `replacesClientId`.
`R6.1.2a` — **Host contract for client identity** (customer decision `D-5`, §17.3): a `ClientId` identifies a **document, not a tab or window**. The host MUST therefore:
(a) allocate a fresh `ClientId` for every document, including every navigation within an existing window;
(b) pass that fresh id as `SwRequest::reserved_client_id` and the id of the document being replaced as `SwRequest::replaces_client_id` on every navigation request handed to `handle_fetch`;
(c) call `on_client_created` with the new id once the navigation commits, and `on_client_unload` for the replaced id;
(d) never reuse an id after `on_client_unload`.
This contract is documented in `docs/host-integration.md` and verified by the host-contract validator of `T-23`.
`R6.1.3` — the registry is keyed by `(StorageKey, scope Url)`; scope URLs are stored in serialized normalized form (no fragment; query preserved).

### 6.2. URL handling and scope matching

`R6.2.1` — all URL parsing uses the `url` crate against a base URL; parse failures are `SwError::InvalidUrl`.
`R6.2.2` — **Scope match** (`#scope-match-algorithm`): a registration whose serialized scope is a **prefix of the serialized client URL** (fragment excluded from the client URL) matches. Longest matching scope wins; ties are impossible (scopes are unique per key). The comparison is over serialized strings, so `/foo` matches `/foobar` — this is intentional and MUST be preserved (a known Spec property).
`R6.2.3` — **Path restriction** (`#path-restriction`): the scope path MUST be prefixed by the script URL's directory path, unless the script response carries `Service-Worker-Allowed: <value>` whose parsed value (against the script URL) is a prefix of the scope. Violation → job fails with `SecurityError` and the message `"The path of the provided scope is not under the max scope allowed"`.
`R6.2.4` — scope and script URL MUST be same-origin with the client that registers them (`SecurityError` otherwise), and their scheme MUST be `http`/`https` (`TypeError` otherwise).
`R6.2.5` — encoded slashes (`%2f`, `%5c`, any case) in the *path* of either URL are rejected with `TypeError`.
`R6.2.6` — helper `pub fn default_scope(script_url: &Url) -> Url` returns the script URL with the last path segment removed (i.e. resolution of `"./"`).

### 6.3. Records

```rust
pub enum WorkerState { Parsed, Installing, Installed, Activating, Activated, Redundant }
pub enum WorkerType { Classic, Module }
pub enum UpdateViaCache { Imports, All, None }          // default Imports

pub struct WorkerRecord {
    pub id: WorkerId,
    pub registration: RegistrationId,
    pub script_url: Url,
    pub worker_type: WorkerType,
    pub state: WorkerState,
    /// Set by `skipWaiting()`.
    pub skip_waiting: bool,
    /// Set once the script has been evaluated at least once; used by importScripts (§11.4.2).
    pub imported_scripts_updated: bool,
    /// `None` until the script has been evaluated once; then whether a `fetch` listener exists.
    pub has_fetch_handler: Option<bool>,
    /// Event types with at least one registered listener at end of initial evaluation.
    pub handled_event_types: SmallVec<[EventType; 8]>,
    /// Runtime status; not persisted.
    pub run_state: RunState,          // NotRunning | Starting | Running | Terminating
    /// Number of unsettled extended lifetime promises + in-flight dispatches (§11.3).
    pub pending_events: u32,
    pub last_activity_ms: u64,
}

pub struct RegistrationRecord {
    pub id: RegistrationId,
    pub storage_key: StorageKey,
    pub scope: Url,
    pub update_via_cache: UpdateViaCache,
    pub installing: Option<WorkerId>,
    pub waiting: Option<WorkerId>,
    pub active: Option<WorkerId>,
    pub last_update_check_ms: Option<u64>,
    pub navigation_preload_enabled: bool,
    pub navigation_preload_header: String,   // default "true"
    /// Set by Unregister while an active worker still controls clients.
    pub uninstalling: bool,
}
```

`R6.3.1` — `WorkerRecord`, `RegistrationRecord`, `ScriptResource` are `Clone`, `Debug`, and (feature `serde`) `Serialize`/`Deserialize`.
`R6.3.2` — states are changed **only** through `SwCore::update_worker_state` / `update_registration_state`, which emit the corresponding notification commands (§7.7). Direct field assignment elsewhere is a defect.

### 6.4. Script resources, byte-for-byte comparison, update-via-cache

```rust
pub struct ScriptResource {
    pub url: Url,
    pub bytes: Rc<[u8]>,
    pub sha256: [u8; 32],
    pub mime: String,                 // essence, lowercased
    pub service_worker_allowed: Option<String>,
    pub is_main: bool,
}
pub type ScriptResourceMap = IndexMap<Url, ScriptResource>;
```

`R6.4.1` — a script response is valid only if: status is an ok status (2xx), the MIME essence is one of `application/javascript`, `application/ecmascript`, `application/x-ecmascript`, `application/x-javascript`, `text/javascript`, `text/ecmascript`, `text/javascript1.0..1.5`, `text/jscript`, `text/livescript`, `text/x-ecmascript`, `text/x-javascript`; otherwise the job fails with `SecurityError` (`"The script has an unsupported MIME type"`).
`R6.4.2` — a redirect response for the **main** script fails the job with `SecurityError`; imported scripts follow the same rule.
`R6.4.3` — **byte-for-byte comparison**: the update job compares the newly fetched main script bytes with the stored bytes of the newest worker's map entry for the same URL. When they are equal AND the imported-script set is unchanged (each stored import URL re-fetched per `updateViaCache` and equal), the job resolves without creating a new worker (`#update-algorithm` step "If newestWorker is not null, ... byte-to-byte"). SHA-256 comparison is permitted as an optimization only when lengths also match; the implementation MUST compare bytes on hash equality (no hash-only decision).
`R6.4.4` — `updateViaCache` maps to the request cache policy passed to `HttpClient`:

| `updateViaCache` | main script | imported scripts |
|---|---|---|
| `imports` (default) | `CachePolicy::NoCache` (bypass HTTP cache) | `CachePolicy::Default` |
| `all` | `CachePolicy::Default` | `CachePolicy::Default` |
| `none` | `CachePolicy::NoCache` | `CachePolicy::NoCache` |

Additionally, when the job's `force_bypass_cache` flag is set (soft update after a 24 h gap, or an explicit `update()` from a page), all script fetches use `CachePolicy::Reload`.
`R6.4.5` — the script resource map of a worker is persisted with the worker and restored on `install`; imported scripts are keyed by their absolute URL.
`R6.4.6` — `max_script_bytes` and `max_imported_scripts` are enforced; violation fails the job with `SwError::QuotaExceeded` → `QuotaExceededError`.

### 6.5. Registry and lookups

```rust
impl SwCore {
    pub fn match_registration(&self, key: &StorageKey, client_url: &Url) -> Option<RegistrationId>;
    pub fn get_registration(&self, key: &StorageKey, scope: &Url) -> Option<RegistrationId>;
    pub fn newest_worker(&self, reg: RegistrationId) -> Option<WorkerId>;   // installing ?? waiting ?? active
    pub fn registrations_for_origin(&self, key: &StorageKey) -> Vec<RegistrationId>;
}
```

`R6.5.1` — `newest_worker` MUST return `installing`, else `waiting`, else `active` (`#get-newest-worker`).
`R6.5.2` — `match_registration` MUST skip registrations whose `uninstalling` flag is set (`#match-service-worker-registration`).

---

## 7. Job queue and lifecycle algorithms

### 7.1. Job model

```rust
pub enum JobKind { Register, Update, Unregister }

pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub storage_key: StorageKey,
    pub scope: Url,
    pub script_url: Option<Url>,          // None for Unregister
    pub worker_type: WorkerType,
    pub update_via_cache: UpdateViaCache,
    pub force_bypass_cache: bool,
    pub client: Option<ClientId>,         // the client that requested it, if any
    pub referrer: Option<Url>,
    /// Promises to settle when the job finishes; a job may accumulate several (see equivalence).
    pub promises: SmallVec<[JobPromiseId; 2]>,
    pub state: JobState,                  // Queued | Running(JobStep) | Finished
}

pub struct JobQueue { scope: Url, entries: VecDeque<Job> }
```

`R7.1.1` — **Schedule Job** (`#schedule-job`): if the queue's last entry is *equivalent* to the new job and that entry has not started, merge the new job's promises into it and drop the new job. Two jobs are equivalent when kind, scope, script url, worker type and `updateViaCache` are equal (per `#job-equivalent`); `unregister` jobs are equivalent when scope matches.
`R7.1.2` — **Run Job** runs the head of the queue; **Finish Job** (`#finish-job`) removes the head and starts the next one, if any. A job MUST settle all its promises exactly once, before Finish Job.
`R7.1.3` — job execution is a state machine: each job has an explicit `JobStep` enum so that a resumption event (`ScriptFetched`, `ScriptEvaluated`, `EventHandled`) is dispatched to the right continuation. No implicit "current job" globals.
`R7.1.4` — a job whose originating client disappears MUST still run to completion; only its promise settlement becomes a no-op.
`R7.1.5` — **Reject Job Promise** MUST settle *all* accumulated promises with the same error; **Resolve Job Promise** with the same registration.

### 7.2. Register (`#register-algorithm`, `#start-register`)

Steps (normative, condensed; the implementer MUST cross-check the Spec text):
1. If `script_url` is failure → reject `TypeError`.
2. If scheme is not `http`/`https` → reject `TypeError`; if origin differs from the client's → reject `SecurityError`.
3. Same checks for `scope`.
4. Schedule the job (§7.1.1).
5. On run: if the client is gone → Finish Job silently. If the job's script url origin ≠ scope origin ≠ client origin → reject `SecurityError`.
6. Let `registration` = existing registration for `(key, scope)`.
7. If it exists and is not `uninstalling` and its **newest worker**'s script url, worker type and `updateViaCache` all equal the job's, then run **Update** with the *existing* registration and this job's promises attached, and return (`#register-algorithm` step 8).
8. Otherwise: if it exists, clear `uninstalling` and set `update_via_cache`; else create a new registration with the job's scope, `updateViaCache` and no workers.
9. Run **Update** (§7.3) for this registration.

`R7.2.1` — `register()` MUST NOT itself fetch anything; all fetching happens inside Update.
`R7.2.2` — creating a registration is persisted only after the Update job has successfully installed a worker (`AD-11`); an Update failure on a *newly created* registration MUST remove the registration from the registry (`#update-algorithm` "If newestWorker is null, invoke Clear Registration").

### 7.3. Update (`#update-algorithm`) and Soft Update (`#soft-update`)

1. Let `newestWorker` = Get Newest Worker.
2. If the job type is `update` and `newestWorker` is not null and its script url ≠ job's script url → reject `TypeError`, Finish Job.
3. Set `force_bypass_cache` when the job says so; compute the main script `CachePolicy` per `R6.4.4`.
4. Issue `CoreCommand::FetchScript` for the main script with header `Service-Worker: script`, `destination = ServiceWorker`, `redirect = Error`, `mode = SameOrigin`, `credentials = SameOrigin` (`Include` when the request is same-origin, per Spec).
5. On response: validate status, MIME (`R6.4.1`), redirect (`R6.4.2`), `Service-Worker-Allowed` path restriction (`R6.2.3`), size (`R6.4.6`). Any failure → Reject Job Promise with the mapped error, and if `newestWorker` is null → Clear Registration; then Finish Job.
6. Set `registration.last_update_check_ms = clock.now_ms()` and persist.
7. If `newestWorker` is not null, its script url equals the job's, its worker type equals the job's, and the fetched main script bytes are byte-for-byte equal to the stored ones **and** all imported scripts re-fetch equal (`R6.4.3`) → Resolve Job Promise with the registration, Finish Job.
8. Otherwise create a new `WorkerRecord` in state `Parsed` and command `EvaluateScript` (which internally performs `importScripts` fetches for classic workers, or module-graph fetches for module workers — §11.4).
9. On `ScriptEvaluated { Err(_) }` → Reject Job Promise with the evaluation error (or `TypeError` for a failed script fetch), mark the new worker `Redundant`, if `newestWorker` is null → Clear Registration, Finish Job.
10. On `ScriptEvaluated { Ok(info) }` → record `info.has_fetch_handler`, `info.handled_event_types`, the final script resource map; run **Install** (§7.4).

`R7.3.1` — **Soft Update** is invoked (a) after a navigation `FetchEvent` has been handled, and (b) after any functional event has been handled, and (c) from `handle_fetch` before dispatch, only when `now_ms - last_update_check_ms > 86_400_000`; in that case `force_bypass_cache` is set. A soft update MUST NOT settle any page-visible promise.
`R7.3.2` — soft updates MUST be deduplicated: at most one queued `update` job per scope (job equivalence handles this).
`R7.3.3` — an `update` job for a registration with no `newestWorker` MUST be a no-op that finishes the job.

### 7.4. Install (`#installation-algorithm`)

1. Set `installingWorker` = the new worker; set registration's `installing` slot; update worker state to `Installing`.
2. Notify `updatefound` on every `ServiceWorkerRegistration` object for this registration (`CoreCommand::NotifyUpdateFound`).
3. Resolve Job Promise with the registration (**before** the install event completes — this is the Spec order and it matters for `register()` semantics).
4. Command `DispatchEvent { worker, InstallEvent }`. The bindings dispatch `install` on the worker's global, collect ELPs, and report `EventHandled { outcome }` where outcome is `Completed`, `Rejected(error)`, or `TimedOut`.
5. On `Rejected`/`TimedOut`: set worker state `Redundant`, clear the `installing` slot, if the registration has no worker at all → Clear Registration; Finish Job.
6. On `Completed`: if the registration's `waiting` worker exists, terminate it and set its state `Redundant`; move installing → waiting; update state to `Installed`; persist (one batch: worker rows, script map, registration slots).
7. If `installingWorker.skip_waiting` is true, or the registration has no active worker **and** no controlled clients → run **Try Activate** (§7.5).
8. Finish Job.

`R7.4.1` — step 3 order (resolve before install completes) MUST be covered by a dedicated test.
`R7.4.2` — the install event's timeout is `install_event_timeout_ms`; expiry is treated as `Rejected` with `AbortError` and MUST reject every outstanding ELP of that event.

### 7.5. Activate (`#activation-algorithm`) and Try Activate (`#try-activate`)

**Try Activate:**
1. If the registration has no `waiting` worker → return.
2. If the registration has an `active` worker that still controls at least one client, and the waiting worker's `skip_waiting` flag is false → return (stay waiting).
3. Otherwise run **Activate**.

**Activate:**
1. If there is an `active` worker: terminate it, set state `Redundant`, and for each client it controls, unset the controller **without** firing `controllerchange` (the Spec keeps controlled clients pointing at the old worker until they navigate; see `R7.5.2`).
2. Move `waiting` → `active`; clear `waiting`; persist.
3. If the new active worker's `skip_waiting` flag is set, for every client whose controller was the previous active worker, set the controller to the new worker and fire `controllerchange` (§7.7.3).
4. Update worker state to `Activating`.
5. Command `DispatchEvent { worker, ActivateEvent }`.
6. On `EventHandled` (any outcome): update worker state to `Activated`, persist, and run **Try Clear Registration** if the registration is `uninstalling`.

`R7.5.1` — activation MUST NOT be blocked by an activate handler rejection; the worker still becomes `Activated` (`#activation-algorithm` last step).
`R7.5.2` — clients acquire a controller **only** at navigation time (`handle_fetch` for a navigation request, §8.1) or through `clients.claim()`/`skipWaiting`; an activation alone MUST NOT retro-control existing clients.
`R7.5.3` — **Try Clear Registration** (`#try-clear-registration`): if the registration has no worker that controls a client and no running worker with pending events, remove it from the registry and delete all its persisted rows and script maps in one batch.

### 7.6. Unregister (`#unregister-algorithm`)

1. If the job's scope origin ≠ client origin → reject `SecurityError`, Finish Job.
2. Let `registration` = Get Registration; if none → resolve `false`, Finish Job.
3. Remove the registration from the "scope to registration map" so that new clients cannot match it; set `uninstalling = true`; persist.
4. Resolve Job Promise with `true`.
5. Run Try Clear Registration; Finish Job.

`R7.6.1` — after step 3, `getRegistration()`/`getRegistrations()`/`match_registration` MUST NOT return the registration, but existing controlled clients keep their controller until unload.

### 7.7. State updates and notifications

`R7.7.1` — **Update Worker State** (`#update-state`): set the state, then command `NotifyWorkerStateChange`; bindings fire `statechange` on every `ServiceWorker` object for that worker, in every realm, as a task (not synchronously inside core).
`R7.7.2` — **Update Registration State** (`#update-registration-state`): when the `installing` slot gains a worker, command `NotifyUpdateFound`; bindings fire `updatefound` on every `ServiceWorkerRegistration` object for that registration.
`R7.7.3` — **Notify Controller Change** (`#notify-controller-change`): bindings fire `controllerchange` on the affected client's `ServiceWorkerContainer`, and the runtime calls `ClientHost::on_controller_changed`.
`R7.7.4` — all three notifications MUST be queued as tasks and delivered in the order the core emitted them.

### 7.8. `skipWaiting` and `clients.claim`

`R7.8.1` — `skipWaiting()`: set `worker.skip_waiting = true`; if the worker is the registration's `waiting` worker → Try Activate; resolve the promise after Try Activate returns (the promise does not wait for the activate event).
`R7.8.2` — `clients.claim()`: for every client of the worker's storage key whose URL matches the registration's scope and whose controller is not this worker, set the controller to this worker and fire `controllerchange`; resolve `undefined`. Rejects with `InvalidStateError` when the calling worker is not the registration's `active` worker.

### 7.9. Running, termination and idleness

```rust
pub enum RunState { NotRunning, Starting, Running, Terminating }
```

`R7.9.1` — **Run Service Worker** (`#run-service-worker`): if `run_state != NotRunning` do nothing; otherwise create a realm, install the SW global surface, evaluate the script (from the stored script resource map, never from the network), and set `Running`. Failure → `Redundant` + all pending dispatches fail.
`R7.9.2` — **Service Worker Has No Pending Events** (`#service-worker-has-no-pending-events`): true when `pending_events == 0` and no dispatch is in flight and no ELP is unsettled.
`R7.9.3` — **Terminate Service Worker** (`#terminate-service-worker`): reject every unsettled ELP with `AbortError`, abandon in-flight dispatches (their `EventHandled` outcome is `Terminated`), drop the realm and all per-realm identity maps, set `run_state = NotRunning`. The worker record and its state (e.g. `Activated`) survive.
`R7.9.4` — the runtime MUST terminate a running worker when: it has no pending events for `worker_idle_timeout_ms`; or a single event exceeds `functional_event_timeout_ms`; or `shutdown()` is called; or the worker becomes `Redundant`.
`R7.9.5` — a terminated worker MUST be startable again on the next event without re-fetching or re-installing.

### 7.10. Client lifecycle hooks

`R7.10.1` — **Handle Service Worker Client Unload** (`#on-client-unload`): when a controlled client unloads, drop it from the controller's client set; if its registration is `uninstalling` and no clients remain → Try Clear Registration; if a waiting worker exists and no clients remain → Try Activate.
`R7.10.2` — **Handle User Agent Shutdown** (`#on-user-agent-shutdown`): for every registration, if `installing` exists → set it `Redundant` and clear the slot; if `waiting` exists and `active` exists → the Spec keeps both; persist the resulting state so that a restart is consistent.

### 7.11. Restart recovery

`R7.11.1` — on `install`, every restored worker gets `run_state = NotRunning` and `pending_events = 0`.
`R7.11.2` — a restored registration with an `installing` worker (a crash mid-install) MUST have that worker discarded (`Redundant`, slot cleared, script map deleted).
`R7.11.3` — a restored registration with neither `waiting` nor `active` MUST be deleted.
`R7.11.4` — recovery MUST be a single storage batch and MUST be idempotent (running it twice yields the same state).

---

## 8. Fetch interception

### 8.1. Handle Fetch (`#handle-fetch`)

Core types:

```rust
pub struct SwRequest {
    pub method: http::Method,
    pub url: Url,
    pub headers: http::HeaderMap,
    pub body: Option<Rc<Vec<u8>>>,
    pub mode: RequestMode,               // SameOrigin | NoCors | Cors | Navigate
    pub credentials: RequestCredentials, // Omit | SameOrigin | Include
    pub cache: CachePolicy,              // Default | NoStore | Reload | NoCache | ForceCache | OnlyIfCached
    pub redirect: RedirectMode,          // Follow | Error | Manual
    pub destination: RequestDestination, // Document | Script | ServiceWorker | Style | Image | Empty | ...
    pub referrer: Referrer,
    pub integrity: String,
    pub keepalive: bool,
    pub is_reload: bool,
    pub client_id: Option<ClientId>,
    pub reserved_client_id: Option<ClientId>,   // navigations
    pub replaces_client_id: Option<ClientId>,   // navigations
}

pub struct SwResponse {
    pub kind: ResponseKind,   // Basic | Cors | Default | Error | Opaque | OpaqueRedirect
    pub url_list: Vec<Url>,
    pub status: u16,
    pub status_text: String,
    pub headers: http::HeaderMap,
    pub body: Rc<Vec<u8>>,
    pub redirected: bool,
    pub timing_allow_passed: bool,
}
```

Algorithm (host calls `SwRuntime::handle_fetch`):
1. If the request's URL scheme is not `http`/`https` → `NotIntercepted`.
2. If `request.destination` is `ServiceWorker` (a script fetch performed by this crate) → `NotIntercepted`.
3. Determine the *client* to match: for a navigation (`mode == Navigate`), match by the request URL; otherwise use `request.client_id`'s controller.
4. For a navigation: `registration = match_registration(key, url)`; if none → `NotIntercepted`. Otherwise if `registration.active` is null → `NotIntercepted`; if the active worker's state is `Activating`, wait for it to become `Activated` (the interception stays `Pending`).
5. For a subresource: if the client has no controller → `NotIntercepted`.
6. Run **Should Skip Event** (`#should-skip-event`): if the worker's `has_fetch_handler == Some(false)` → `NotIntercepted` (and, for a navigation, still trigger a soft update per `R7.3.1`).
7. If the worker is not running → Run Service Worker.
8. Create the `FetchEvent` with `request` (a `Request` object with the same values, `mode`/`credentials`/`destination` preserved), `clientId` (empty string for navigations), `resultingClientId` (the reserved client id for navigations, empty otherwise), `replacesClientId`, and `preloadResponse`.
9. Dispatch it. If `respondWith` was not called and dispatch completed → `FallbackToNetwork`. If the event handler threw and `respondWith` was not called → `Failed(NetworkError::HandlerThrew)`.
10. If `respondWith` was called, wait for its promise. Validate per §8.2 and produce `Responded` or `Failed`.
11. When the response is delivered for a **navigation**, set the resulting client's controller to the active worker (this is how clients become controlled) and record it in the worker's client set.
12. After the event has been handled, run Soft Update if the 24 h rule applies (`R7.3.1`).

`R8.1.1` — steps 1–6 MUST be synchronous and allocation-light; `handle_fetch` returning `NotIntercepted` must not create a realm, a JS object, or a task.
`R8.1.2` — `has_fetch_handler` is computed at the end of the worker's *initial script evaluation* by inspecting the global's listener list for `fetch` plus `onfetch`; it MUST be persisted with the worker so that step 6 works before the worker is started.
`R8.1.3` — the interception handle MUST be cancellable; on cancel the runtime marks the event abandoned and rejects `handled`.

### 8.2. `respondWith` resolution

`R8.2.1` — the argument is resolved as a promise; a non-`Response` fulfilment value → `Failed(NetworkError::TypeError)` and the `FetchEvent`'s `handled` promise rejects with `TypeError`.
`R8.2.2` — a rejected promise → `Failed(NetworkError::HandlerRejected)`.
`R8.2.3` — a response whose body has already been used (`bodyUsed`) → `TypeError`.
`R8.2.4` — response type rules: `opaque` is allowed only when the request mode is `no-cors`; `opaqueredirect` only when the request mode is `navigate` and redirect mode is `manual`; `cors`/`basic`/`default` always allowed; `error` → network error.
`R8.2.5` — a response with status 0 that is not `opaque`/`opaqueredirect`/`error` → `TypeError`.
`R8.2.6` — a body exceeding `max_body_bytes` → `Failed(NetworkError::BodyTooLarge)`.
`R8.2.7` — the `handled` promise resolves after a successful response is handed to the host, and rejects with the same error otherwise; it MUST settle exactly once.
`R8.2.8` — the ELP created by `respondWith` keeps the worker alive until the response is delivered.

### 8.3. Navigation preload (feature `navigation-preload`)

`R8.3.1` — when the registration has navigation preload enabled, the request is a navigation, and the request method is `GET`, the runtime MUST issue a *parallel* network request through `HttpClient` before dispatching the event, adding the header `Service-Worker-Navigation-Preload: <headerValue>`.
`R8.3.2` — `FetchEvent.preloadResponse` resolves with the resulting `Response`, or with `undefined` when preload is disabled/not applicable, and rejects with a `TypeError` when the preload request fails.
`R8.3.3` — if the event never consumes `preloadResponse` and `respondWith` produced a response, the preload request MUST be cancelled (`HttpClient::cancel`).

### 8.4. Client id assignment

`R8.4.1` — navigations carry a host-assigned `reserved_client_id`; the runtime surfaces it as `resultingClientId` and MUST NOT invent ids (`R6.1.2a`). A navigation request that arrives without a `reserved_client_id` MUST be reported through `SwObserver` as a host-contract violation and treated as `NotIntercepted`, never silently patched up.
`R8.4.2` — `replacesClientId` is the id of the client being replaced by the navigation (empty string when none).
`R8.4.3` — for subresource requests `clientId` is the requesting client's id and `resultingClientId` is `""`.

### 8.5. Requests that never reach a worker

`R8.5.1` — script fetches performed by the update algorithm (`Service-Worker: script`), requests whose destination is `ServiceWorker`, requests with non-http(s) schemes, and `fetch()` calls made *inside* a SW realm MUST bypass interception.
`R8.5.2` — the host MAY mark a request `SwRequest::skip_service_worker` (a bool field); the runtime MUST honour it and return `NotIntercepted`.

---

## 9. Cache API

### 9.1. Storage model

`R9.1.1` — cache storage is scoped by `StorageKey`; cache names are arbitrary `DOMString`s, ordered by creation.
`R9.1.2` — a cache entry stores: request (method, url, headers, body-less), response (kind, url list, status, status text, headers, body bytes), the `Vary` header values captured at insertion time, insertion order index, and byte size.
`R9.1.3` — entry lookup keys are `(cache_id, request_url_without_fragment, method)`; multiple entries with the same key are allowed and distinguished by `Vary` (`R9.2.3`).

### 9.2. Query Cache (`#query-cache`) and Request Matches Cached Item (`#request-matches-cached-item`)

```rust
pub struct CacheQuery {
    pub request: Option<CacheRequestKey>,  // None => all entries (used by keys())
    pub ignore_search: bool,
    pub ignore_method: bool,
    pub ignore_vary: bool,
}
```

`R9.2.1` — with `ignore_search`, the query URL and the stored URL are compared with their query strings removed.
`R9.2.2` — without `ignore_method`, a request whose method is neither `GET` nor `HEAD` matches nothing.
`R9.2.3` — without `ignore_vary`, for each field name in the stored response's `Vary` header: `*` never matches; otherwise the query request's header value for that name MUST equal the stored request's value (byte equality after header normalization); a missing header on both sides counts as equal.
`R9.2.4` — results MUST be returned in insertion order.

### 9.3. Batch Cache Operations (`#batch-cache-operations`) and the `Cache` methods

`R9.3.1` — `put(request, response)` rejects with `TypeError` when: the request method is not `GET`; the request URL scheme is not `http`/`https`; the response status is `206`; the response's `Vary` header contains `*`; the response body is already used. Opaque responses ARE allowed.
`R9.3.2` — `put` is a delete-then-insert batch: all entries matching the request (with `ignore_vary = false`) are removed, then the new entry is inserted, atomically.
`R9.3.3` — `add(request)` = `fetch` + `put`; rejects with `TypeError` when the response status is not ok (2xx) or the response type is `opaque`/`error`.
`R9.3.4` — `addAll(requests)` performs all fetches, then one atomic batch; duplicate request keys in the list → `InvalidStateError`; any failing response → `TypeError` and **no** entries written.
`R9.3.5` — `delete(request, options)` resolves `true` when at least one entry was removed.
`R9.3.6` — `match`/`matchAll` return cloned responses with fresh bodies; `keys` returns cloned requests.
`R9.3.7` — `CacheStorage.match` searches caches in creation order (or only `options.cacheName`), returning the first match.
`R9.3.8` — `CacheStorage.delete(name)` resolves `false` for an unknown name; open `Cache` objects for a deleted cache MUST keep working on a detached snapshot? **No** — they MUST reject subsequent operations with `InvalidStateError`. This is a deliberate deviation from browsers (documented in `docs/compat.md`) that keeps the storage model simple.
`R9.3.9` — quota: when `cache_quota_bytes` is set and a batch would exceed it, the batch fails with `QuotaExceededError` and writes nothing.

---

## 10. Messaging

### 10.1. Queues and delivery

`R10.1.1` — every client has a **message queue** owned by the runtime; every running worker has one too.
`R10.1.2` — a message sent to a *not running* worker MUST start the worker (Run Service Worker) and be delivered after its script evaluation completes; the message keeps the worker alive (`pending_events += 1` until the ELPs of the resulting event settle).
`R10.1.3` — a client's queue starts **disabled**; it is enabled on the first of: setting `navigator.serviceWorker.onmessage`, `addEventListener("message", …)`, or `startMessages()`. Messages queued before enabling MUST be delivered, in order, once enabled.
`R10.1.4` — delivery order is FIFO per (source, destination) pair and MUST be preserved across worker restarts for messages queued before the restart.
`R10.1.5` — messages to a `redundant` worker throw `InvalidStateError` at `postMessage` time (`R5.1.15`).

### 10.2. Event construction

`R10.2.1` — client → worker produces an `ExtendableMessageEvent` with `source` = a `Client` object for the sending client, `origin` = the sender's origin, and `ports` = the ports transferred with the message (§10.4).
`R10.2.2` — worker → client produces a `MessageEvent` (not extendable) dispatched on the client's `ServiceWorkerContainer`, with `source` = a `ServiceWorker` object for the sending worker.
`R10.2.3` — worker → worker (via `ServiceWorker.postMessage` from inside a SW realm) produces an `ExtendableMessageEvent` with `source` = a `ServiceWorker` object.
`R10.2.4` — a deserialization failure MUST fire `messageerror` (same interface, `data = null`) instead of `message`.

### 10.3. Structured clone

`R10.3.1` — serialization happens **synchronously** in the sender's realm, inside the `postMessage` call, so that `DataCloneError` is thrown synchronously.
`R10.3.2` — with feature `runtime-interop`, use `boa_wintertc::store::JsValueStore::try_from_js(value, ctx, transfer)`; without it, use the crate's fallback serializer supporting: primitives, `String`, `Boolean`, `Number`, `BigInt`, `Date`, `RegExp`, `Array`, plain `Object`, `Map`, `Set`, `ArrayBuffer`, TypedArrays, `DataView`, `Error` objects, and cyclic references. Unsupported values → `DataCloneError`.
`R10.3.3` — transfer lists are accepted; transferring an `ArrayBuffer` MUST detach it in the source realm, and transferring a `MessagePort` MUST detach it (§10.4.3).

### 10.4. `MessageChannel` / `MessagePort`

`R10.4.1` — a `MessageChannel` creates two entangled ports. Entanglement is tracked by the runtime as a `PortPair { a: PortId, b: PortId }` in a table that is independent of realms, so a port may be transferred to a SW realm while its twin stays in the client realm.
`R10.4.2` — `port.postMessage(v)` serializes `v` in the sender's realm (`R10.3.1`) and enqueues it on the **twin** port's queue. Delivery happens during `pump`, in the twin's realm, as a `MessageEvent` dispatched on the twin port. FIFO per port is guaranteed.
`R10.4.3` — transferring a port detaches it: the source-realm object becomes unusable (`postMessage` on it throws `InvalidStateError`, per HTML), its queued-but-undelivered messages move with it, and a new `MessagePort` object is created in the destination realm.
`R10.4.4` — a message sent to a port whose twin lives in a not-running worker MUST start that worker and keep it alive until the resulting event's ELPs settle, exactly like `R10.1.2`.
`R10.4.5` — `port.close()` and the destruction of a realm detach the pair; subsequent `postMessage` calls are silently dropped (per HTML), and the twin receives no further messages.
`R10.4.6` — a port transferred to a worker that is later terminated MUST NOT leak: the port table entry is dropped with the realm, its twin observes the port as closed.
`R10.4.7` — with feature `runtime-interop`, port transfer MUST be checked against `boa_wintertc::store`'s `is_transferable` gate; if that gate does not accept `MessagePort` objects, ports MUST be serialized by this crate's own transfer path and MUST NOT be passed through `JsValueStore` (verified by a test that transfers a port with the feature both on and off).

---

## 11. Integration with the Boa event loop

### 11.1. Realms and jobs

`R11.1.1` — a running worker owns: a `Realm`, an identity map, a `JsObject` for its global, and a list of live `JsPromise` handles for ELPs. All of these are dropped by Terminate Service Worker.
`R11.1.2` — every job the runtime enqueues on behalf of a worker MUST be created with `NativeAsyncJob::with_realm(..., worker_realm.clone())` or wrapped in `with_realm` (`R3.4.3`), so that promise reactions run in the right realm.
`R11.1.3` — the runtime MUST NOT install its own `JobExecutor`; it works with whatever executor the host configured. If the host uses `Context::run_jobs`, `pump` calls it in phase 5 (`R3.4.6`); if the host drives jobs itself, `pump` MUST still be correct (phase 5 becomes a no-op). This is decided by `SwConfig::drive_engine_jobs: bool` (default `true`).

### 11.2. Scheduler

`R11.2.1` — `pump` implements the phase order of `R3.4.6`.
`R11.2.2` — timers (idle timeout, event timeout, ELP timeout) are stored in a monotonic min-heap; `PumpOutcome::Idle { next_deadline_ms }` reports the earliest deadline so a host can sleep.
`R11.2.3` — the scheduler MUST be starvation-free: functional events for different workers are dispatched round-robin; the job queue of one scope never blocks another scope's queue.

### 11.3. Extended lifetime promises

`R11.3.1` — `waitUntil(p)`/`respondWith(p)` register `p` via `JsPromise::then` with native handlers that report settlement to the runtime, incrementing/decrementing `WorkerRecord::pending_events`.
`R11.3.2` — an event is "active" from the start of its dispatch until: dispatch returned **and** all ELPs registered so far have settled. Registering a new ELP while at least one is unsettled extends the window; registering after the window closed throws `InvalidStateError`.
`R11.3.3` — the aggregate outcome reported as `EventOutcome` is `Completed` when every ELP fulfilled, `Rejected` when at least one rejected (first rejection wins for the error value), `TimedOut` on timeout, `Terminated` when the worker was terminated.
`R11.3.4` — an event with **no** ELPs completes as soon as dispatch returns, with outcome `Completed` (or `Rejected` when a listener threw and `install`/`activate` semantics require it — for `install` a thrown listener error does **not** fail installation unless it propagates through an ELP; follow the Spec).
`R11.3.5` — ELP bookkeeping MUST be leak-free: a worker terminated with unsettled ELPs must have its counters reset and its `then` handlers made inert.

### 11.4. Script evaluation

`R11.4.1` — classic worker: concatenation is not used; the main script is compiled and evaluated as a `Script` in the SW realm with the script URL as its source path.
`R11.4.2` — `importScripts(...urls)`: resolve each URL against the worker's script URL; if the worker's script resource map already contains it, evaluate the stored bytes; otherwise (a) if `imported_scripts_updated` is true → throw `NetworkError`; (b) else fetch it through `HttpClient` with `destination = Script` and the `updateViaCache`-derived policy, validate MIME/status/redirect, store it in the map, and evaluate. Fetching inside `importScripts` is **synchronous from the script's point of view**, which the single-threaded design cannot provide: therefore the implementation MUST pre-fetch imports (see `R11.4.4`).
`R11.4.3` — module worker (feature `module-workers`): the module graph is fetched through `HttpClient`, every module response validated like a script resource, stored in the script resource map keyed by absolute URL, then linked and evaluated with Boa's module machinery using a `ModuleLoader` that serves **only** from the script resource map (no network access during linking).
`R11.4.4` — because both cases need the whole script set before evaluation, evaluation is a two-phase command: `CoreCommand::CollectScripts { worker }` (the runtime resolves the transitive import set by static pre-scanning for module workers, and for classic workers by an *evaluation-abort-and-retry* loop: evaluate, and if `importScripts` hits an unknown URL, abort with a resumable marker, fetch, and re-evaluate from the start) followed by `CoreCommand::EvaluateScript { worker }`. The retry loop MUST be bounded by `max_imported_scripts` and MUST be documented in `docs/compat.md` as an observable deviation (side effects before the failing `importScripts` run twice). Hosts that cannot tolerate this MAY declare imports up front via `SwConfig::pre_declared_imports`.
`R11.4.5` — evaluation errors (parse or runtime) are reported as `ScriptEvalError` with the JS error's message and stack, and MUST NOT propagate as Rust panics.

### 11.5. Termination and budget

`R11.5.1` — `SwRuntime` exposes `set_interrupt_hook(Box<dyn Fn(&WorkerId) -> bool>)`; when the engine gains an interrupt facility, the runtime wires it so that a worker exceeding `functional_event_timeout_ms` is interrupted. Until then, timeouts take effect only between jobs.
`R11.5.2` — termination MUST be safe at any point where the runtime holds no `&mut Context` borrow into that realm.

---

## 12. Error model and DOMException mapping

```rust
#[derive(Debug, thiserror::Error)]
pub enum SwError {
    #[error("invalid URL: {0}")] InvalidUrl(String),
    #[error("cross-origin operation is not allowed")] CrossOrigin,
    #[error("insecure context")] InsecureContext,
    #[error("path restriction violated")] PathRestriction,
    #[error("unsupported script MIME type: {0}")] BadScriptMime(String),
    #[error("script fetch failed: {0}")] ScriptFetch(String),
    #[error("script redirected")] ScriptRedirect,
    #[error("script evaluation failed: {0}")] ScriptEval(String),
    #[error("install failed: {0}")] InstallFailed(String),
    #[error("registration is invalid or removed")] InvalidState,
    #[error("operation aborted")] Aborted,
    #[error("timed out")] TimedOut,
    #[error("network error: {0}")] Network(String),
    #[error("quota exceeded")] QuotaExceeded,
    #[error("storage failure: {0}")] Storage(String),
    #[error("value cannot be cloned")] DataClone,
    #[error("not supported: {0}")] NotSupported(String),
    #[error("globals already defined: {name}")] ConflictingGlobals { name: String },
    #[error("runtime already installed")] AlreadyInstalled,
    #[error("re-entrant pump")] Reentrant,
    #[error("internal invariant violated: {0}")] Internal(String),
}
```

Mapping (normative; `boa_sw` MUST implement `fn to_js(&self, ctx) -> JsValue` exactly per this table):

| `SwError` | JS exception | Typical message |
|---|---|---|
| `InvalidUrl`, `DataClone`* | `TypeError` (*`DataCloneError` for clone failures) | `"Failed to parse URL"` |
| `CrossOrigin` | `SecurityError` | `"The origin of the provided scriptURL does not match the current origin"` |
| `InsecureContext` | `SecurityError` | `"Service workers are only available in secure contexts"` |
| `PathRestriction` | `SecurityError` | `"The path of the provided scope is not under the max scope allowed"` |
| `BadScriptMime` | `SecurityError` | `"The script has an unsupported MIME type"` |
| `ScriptFetch`, `ScriptRedirect`, `Network` | `TypeError` | `"Failed to fetch a service worker script"` |
| `ScriptEval`, `InstallFailed` | `TypeError` (the underlying JS error value is used when available) | — |
| `InvalidState` | `InvalidStateError` | — |
| `Aborted`, `TimedOut` | `AbortError` | — |
| `QuotaExceeded` | `QuotaExceededError` | — |
| `Storage`, `Internal` | `UnknownError` | — |
| `NotSupported` | `NotSupportedError` | — |
| `ConflictingGlobals`, `AlreadyInstalled`, `Reentrant` | not reachable from JS (host-side `Result`) | — |

`R12.1` — every `DOMException` thrown MUST carry the legacy `code` value for its name where one exists.
`R12.2` — errors originating in JS (a rejected `waitUntil` promise) MUST be propagated as the original JS value, not re-wrapped.

---

## 13. Security, privacy, quotas

`R13.1` — registration requires a potentially trustworthy origin (`https`, `wss`, `file`, `http://localhost`, `http://127.0.0.1`, `http://[::1]`) unless `allow_insecure_origin` is set. The check MUST live in one function (`is_potentially_trustworthy`) with unit tests.
`R13.2` — storage keys isolate everything: registrations, script maps, caches. A lookup MUST never cross keys, and the SQLite schema MUST enforce this with the key as the first column of every primary key.
`R13.3` — scripts are never executed from the network directly: they are stored first, then evaluated from storage, so what runs is exactly what was compared byte-for-byte.
`R13.4` — the `Service-Worker: script` request header MUST be set on every script fetch so servers can distinguish them; `Service-Worker-Allowed` is the only response header that widens scope.
`R13.5` — cache storage MUST NOT store responses whose request URL scheme is not http(s), and MUST NOT expose the contents of an `opaque` response to script (status 0, empty headers, empty body).
`R13.6` — a SQLite database opened by another process MUST fail with `SwError::Storage` (advisory lock); no silent multi-process sharing.
`R13.7` — quota accounting includes cache bodies + headers + script bytes; `usage()` MUST be O(1) (maintained counter), not a full scan.
`R13.8` — no data is logged at `tracing` level `info` or below that contains response bodies, cookies or `Authorization` headers.

---

## 14. Non-functional requirements

### 14.1. Performance targets

Measured on a modern x86-64 laptop, release build, in-memory backend, single storage key, warm process:

| Operation | Target |
|---|---|
| `handle_fetch` returning `NotIntercepted` | ≤ 2 µs, zero allocations |
| `handle_fetch` → `Responded` for a cached response, worker already running | ≤ 250 µs |
| Cold start of a worker (realm creation + 20 KB script evaluation) | ≤ 8 ms |
| `cache.match` on a cache with 10 000 entries | ≤ 200 µs |
| `cache.put` of a 100 KB response (SQLite) | ≤ 3 ms |
| Registration (register → activated), memory backend, trivial script | ≤ 15 ms |
| `pump` with no work | ≤ 1 µs |

`R14.1.1` — benchmarks (`criterion`) exist for each row and run in CI on a schedule; a regression > 25 % fails the benchmark gate.

### 14.2. Memory

`R14.2.1` — a terminated worker releases its realm and all associated memory: a test that registers, starts, terminates 100 workers MUST show no growth in Boa's heap statistics beyond a documented constant.
`R14.2.2` — script bytes are stored once (`Rc<[u8]>`) and shared between the script map, the storage batch and the evaluation.
`R14.2.3` — cache reads MUST NOT hold the whole cache in memory: `cache_query` returns ids, and bodies are fetched per entry.

### 14.3. Observability

`R14.3.1` — with feature `tracing`, spans: `sw.job` (kind, scope), `sw.worker.start`, `sw.event` (type, worker), `sw.fetch` (url, outcome), `sw.storage.batch` (ops, bytes). Events at `debug` for state transitions, `warn` for timeouts and terminations.
`R14.3.2` — `SwObserver` receives the same information as structured enum values so hosts can build UIs (a DevTools-like panel).
`R14.3.3` — `SwRuntime::registrations()` returns a snapshot suitable for rendering `chrome://serviceworker-internals`-style diagnostics.

---

## 15. Testing and quality gates

### 15.1. Levels

1. **Unit tests** in every module (core algorithms, URL/scope matching, header/vary matching, guards).
2. **Core state-machine tests**: drive `SwCore` with a scripted `CoreEvent` sequence and assert the exact `Vec<CoreCommand>` produced. These are the primary correctness tests for §7 and MUST cover every algorithm branch.
3. **Integration tests** with a real `Context`: register/install/activate/fetch/message/cache flows against fake host traits.
4. **Differential tests**: memory backend vs SQLite backend, 10 000 randomized operation sequences, identical observable results.
5. **Conformance**: WPT subset (§15.3).
6. **Property tests** (`proptest`): scope matching, `Vary` matching, header ordering, cache query determinism, URL round-trips.
7. **Fuzz** (`cargo-fuzz`): script-response validation, header parsing, cache query, structured-clone fallback serializer.

### 15.2. Coverage

`R15.2.1` — line coverage ≥ 90 % for `boa_sw_core`, ≥ 80 % for `boa_sw`, `boa_sw_fetch`, `boa_sw_dom`.
`R15.2.2` — every requirement id `Rx.y.z` in this document appears in `docs/traceability.md` with at least one test name.

### 15.3. WPT

`R15.3.1` — the harness (`boa_sw_wpt`) runs the `service-workers/service-worker/` directory of web-platform-tests with a shim for `testharness.js`, a fake HTTP server serving the WPT resources from disk, and a fake client host that models one window client.
`R15.3.2` — tests requiring `document`, iframes, `window.open`, workers, or Push/Notifications are excluded through `expectations.json` with a reason string per file.
`R15.3.3` — the acceptance contract is **two-part** (customer decision `D-4`, §17.3):
(a) **≥ 70 %** of runnable subtests PASS at `T-24` and **≥ 85 %** at release, with zero CRASH/TIMEOUT; and
(b) the following files MUST be **100 % green with no entries in `expectations.json`** — a percentage alone is gameable by filtering, this list is not:

```
registration-basic.https.html            registration-scope.https.html
registration-script.https.html           registration-script-url.https.html
registration-updateviacache.https.html   update-after-navigation-fetch-event.https.html
update-registration-with-type.https.html install-event-type.https.html
activate-event-after-install-state-change.https.html
activation.https.html                    activation-after-registration.https.html
skip-waiting.https.html                  skip-waiting-installed.https.html
skip-waiting-without-client.https.html   claim-not-using-registration.https.html
claim-using-registration.https.html      claim-with-redirect.https.html
controller-on-load.https.html            controller-on-reload.https.html
fetch-event.https.html                   fetch-event-respond-with-response.https.html
fetch-event-network-error.https.html     fetch-request-fallback.https.html
fetch-request-no-freshness-headers.https.html
unregister.https.html                    unregister-then-register.https.html
getregistration.https.html               getregistrations.https.html
ready.https.html                         postmessage.https.html
extendable-message-event.https.html      multiple-update.https.html
```
plus every file under `cache-storage/` that does not require `Blob` or streaming bodies.
The list lives in `crates/boa_sw_wpt/priority.txt`; CI fails if any file in it has a non-PASS subtest.
`R15.3.4` — `expectations.json` is checked in; a newly passing test that is still listed as expected-fail fails CI (no silent drift).

### 15.4. Determinism and ordering

`R15.4.1` — a dedicated suite drives the runtime with a `FakeClock`, a scripted `HttpClient` and a scripted `ClientHost`, and asserts a **golden event log** (`insta` snapshots) for: register→install→activate; update with byte-identical script; update with changed script; skipWaiting; claim; unregister with a controlled client; fetch fallback; fetch respondWith; message queueing before `startMessages`.
`R15.4.2` — running any of these suites twice MUST produce byte-identical logs.

### 15.5. Robustness

`R15.5.1` — fault injection: every `SwStorage` method can be made to fail; the runtime MUST surface an error, never panic, and MUST leave the registry consistent.
`R15.5.2` — crash consistency: 200 randomized runs where the process is "killed" (storage handle dropped) at a random batch boundary; after recovery (§7.11) the registry MUST satisfy the invariants checker.
`R15.5.3` — an invariants checker (`SwCore::check_invariants()`) is available in debug builds and asserted after every test step: no worker in two slots, no registration without workers unless `uninstalling`, no running worker that is `Redundant`, counters non-negative, ids unique.

### 15.6. CI

`R15.6.1` — matrix: 3 OSes × (default features, `--no-default-features`, `--all-features`, `--features sqlite-backend`), plus a `wasm32-unknown-unknown` build of `boa_sw_core`.
`R15.6.2` — gates: `fmt`, `clippy -D warnings`, `cargo deny check`, tests, coverage thresholds, WPT run with expectations, benchmark gate (scheduled).
`R15.6.3` — every task's acceptance criteria MUST be verifiable by running commands listed in that task; CI runs exactly those commands.

---

## 16. Work breakdown: milestones and tasks

### 16.0. How work orders are executed

`R16.0.1` — one task = one branch (`task/t-01`, `task/t-02`, …) = one handoff document `docs/reviews/T-<NN>-handoff.md`. An implementer takes **exactly one** task, implements it fully, and stops.
`R16.0.2` — a task is complete only when **all** its acceptance criteria pass, `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features` and `cargo deny check` are clean, and the handoff document is written.
`R16.0.3` — an implementer MUST NOT: change public signatures defined in this document, add dependencies outside §2.3, implement parts of another task ("while I'm here"), or introduce `unsafe`. Any of these requires an escalation entry in `docs/QUESTIONS.md` and a stop.
`R16.0.4` — where this document gives a signature, the code MUST match it character-for-character except for lifetimes/`where` clauses required by the compiler. Where it gives algorithm steps, the code MUST have a comment referencing the step number on each corresponding block.
`R16.0.5` — every task carries a **diff budget**. Exceeding it by more than 50 % means the task is mis-scoped: stop and escalate.
`R16.0.6` — Appendix F is the work-order template. Before starting a task, expand it into `tasks/<NN>_TASK_<NAME>.md` (this can be done by the architect or by the implementer as step 0), copying the relevant normative text so the implementer does not have to interpret this document.

### 16.1. Milestone overview

| Milestone | Tasks | Outcome | Gate |
|---|---|---|---|
| **M0. Foundations** | T-01 … T-03 | Workspace, error model, URL/scope layer, ids | CI green, unit tests |
| **M1. Core model & lifecycle** | T-04 … T-08 | `SwCore` state machine: jobs, register/update/install/activate/unregister, storage traits, memory backend | State-machine test suite green, invariants checker |
| **M2. Fetch & cache core** | T-09 … T-10 | Fetch data model, Handle Fetch decision logic, Cache algorithms | Core-level fetch/cache suites green |
| **M3. Boa bindings** | T-11 … T-21, T-27 | Everything visible from JavaScript | Acceptance scenario (Appendix D) passes on memory backend |
| **M4. Persistence & host integration** | T-22 … T-23 | SQLite backend, restart recovery, host API polish | Differential tests, crash-consistency suite |
| **M5. Conformance & release** | T-24 … T-26 | WPT, hardening, docs, release | WPT ≥ 85 %, all §17 criteria |

Parallelism: T-09/T-10 may run in parallel with T-06/T-07 once T-05 is accepted. T-11 (DOM shim) depends only on T-01 and may start at any time. T-22 (SQLite) may run in parallel with all of M3 once T-08 is accepted. T-24 requires M3 complete.

Estimated effort (senior Rust engineer, or an agent under review): M0 ≈ 1.5 weeks, M1 ≈ 5, M2 ≈ 2, M3 ≈ 12 (including `T-27`), M4 ≈ 4, M5 ≈ 4.5 — **≈ 29 person-weeks** total.

---

### M0 — Foundations

#### T-01 — Workspace skeleton, toolchain, CI

| | |
|---|---|
| Milestone | M0 |
| Crates | workspace root, all crate stubs |
| Prerequisites | — |
| Normative sections | §2, §15.6 |
| Diff budget | ~700 lines |

**Implement**
1. `Cargo.toml` workspace with the seven crates of §2.2, `resolver = "3"`, `[workspace.package]` (version `0.1.0`, edition 2024, rust-version 1.91.0, licence `MIT OR Apache-2.0`), `[workspace.dependencies]` pinning every crate of §2.3, and `[workspace.lints]` per §2.1.
2. `rust-toolchain.toml`, `clippy.toml` (`avoid-breaking-exported-api = false`), `deny.toml` (licence allow-list `MIT`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unlicense`, `Zlib`, `CC0-1.0`).
3. Crate stubs with `lib.rs` containing `#![deny(unsafe_code)] #![warn(missing_docs)]`, a module-level doc comment stating the crate's role in the pipeline, and a `README.md` per crate.
4. Feature declarations exactly as in §2.4 (features may gate nothing yet, but must exist and compile).
5. `docs/SPEC_REVISION.md` recording the Spec URL, its "Latest published version" date, and the editor's-draft commit SHA used; `docs/DECISIONS.md` seeded with `AD-1 … AD-12` from §3.2 (one paragraph each); empty `docs/QUESTIONS.md`, `docs/compat.md` (pre-filled with §1.4), `docs/traceability.md` (header + empty table).
6. GitHub Actions workflow implementing the matrix of `R15.6.1` and the gates of `R15.6.2` (coverage/WPT/bench steps may be `continue-on-error` placeholders that print "not yet implemented").
7. `AGENTS.md` is already delivered; verify it is referenced from every crate README.

**Acceptance criteria**
1. `cargo build --workspace --all-features` and `cargo build --workspace --no-default-features` succeed.
2. `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo deny check` are clean.
3. `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` succeeds.
4. `cargo tree -p boa_sw_core | grep -E 'boa_engine|boa_gc|boa_runtime|boa_wintertc'` produces **no output** (`R2.2.2`).
5. All files listed in items 1–6 exist; `docs/DECISIONS.md` contains twelve AD entries.

**Do not do**: any model, algorithm or binding code.

---

#### T-02 — Error taxonomy, ids, storage key, clock and observer traits

| | |
|---|---|
| Milestone | M0 |
| Crates | `boa_sw_core` |
| Prerequisites | T-01 |
| Normative sections | §4.2.1, §4.2.5, §6.1, §12 |
| Diff budget | ~600 lines |

**Implement**
1. `error.rs`: `SwError` exactly as in §12, plus `StorageError`, `NetworkError`, `ScriptFetchError`, `ScriptEvalError`, and `From` conversions into `SwError`. Add `impl SwError { pub fn dom_name(&self) -> Option<&'static str>; pub fn default_message(&self) -> String; }` returning the values from the §12 table.
2. `ids.rs`: the id newtypes of §6.1 with `Display`, an `IdAllocator { next: u64 }` supporting `restore_from(max: u64)`.
3. `key.rs`: `StorageKey` with `from_origin(&Url)`, `as_str`, `is_potentially_trustworthy(&Url) -> bool` (`R13.1`).
4. `clock.rs`: the `Clock` trait (§4.2.1) plus `FakeClock` (behind `cfg(feature = "test-util")`) with `advance_ms`.
5. `observe.rs`: `ObserverEvent` enum (job started/finished, worker state changed, event dispatched/handled, fetch decision, storage batch applied, worker terminated) and the `SwObserver` trait.

**Tests**
- `dom_name`/`default_message` table test covering every `SwError` variant (exhaustive `match`, so adding a variant breaks the test).
- `is_potentially_trustworthy`: https, wss, file, localhost, 127.0.0.1, [::1], http://example.com (false), data:, blob:.
- `IdAllocator::restore_from` continues above the maximum.

**Acceptance criteria**
1. Every `SwError` variant has a mapping entry; the exhaustiveness test compiles without a wildcard arm.
2. `cargo test -p boa_sw_core` green; coverage of `error.rs`, `key.rs` ≥ 95 %.
3. No `boa_*` dependency (re-check criterion 4 of T-01).

**Do not do**: any registration/job logic.

---

#### T-03 — URL layer: parsing, scope matching, path restriction

| | |
|---|---|
| Milestone | M0 |
| Crates | `boa_sw_core` |
| Prerequisites | T-02 |
| Normative sections | §6.2 |
| Diff budget | ~500 lines |

**Implement**
1. `url_util.rs` with:
```rust
pub fn parse_with_base(input: &str, base: &Url) -> Result<Url, SwError>;
pub fn serialize_exclude_fragment(u: &Url) -> String;
pub fn default_scope(script_url: &Url) -> Url;                       // R6.2.6
pub fn has_encoded_slash(u: &Url) -> bool;                           // R6.2.5
pub fn same_origin(a: &Url, b: &Url) -> bool;
pub fn scope_matches(scope: &Url, client_url: &Url) -> bool;         // R6.2.2
pub fn path_restriction_ok(scope: &Url, script_url: &Url, allowed: Option<&str>)
    -> Result<(), SwError>;                                          // R6.2.3
pub fn is_javascript_mime(essence: &str) -> bool;                    // R6.4.1
```
2. Exhaustive doc comments quoting the Spec rule each function implements.

**Tests**
- Table tests for `scope_matches`: `/` vs `/x`, `/foo` vs `/foobar` (true, documented), `/foo/` vs `/foo` (false), query strings, different origins, trailing-slash behaviour.
- `path_restriction_ok`: script at `/js/sw.js` with scope `/` fails; with `Service-Worker-Allowed: /` succeeds; with `Service-Worker-Allowed: /a` and scope `/b` fails; relative allowed value resolution.
- `has_encoded_slash`: `%2f`, `%2F`, `%5c`, `%5C` in path only (not in query).
- Property test: `scope_matches(default_scope(&s), &s)` is true for any parseable script URL `s`.
- `is_javascript_mime` covers the full list of `R6.4.1` and rejects `text/plain`, `application/json`, `text/javascript; charset=utf-8` (essence must be extracted by the caller — document this).

**Acceptance criteria**
1. All listed tests exist and pass; ≥ 30 table cases for `scope_matches` and `path_restriction_ok` combined.
2. `cargo test -p boa_sw_core` green, module coverage ≥ 95 %.
3. No function in this module allocates a `String` on the matching hot path (`scope_matches` takes `&Url`, compares serialized prefixes without new allocations) — asserted by a `#[bench]`-style test using `std::hint::black_box` and a documented note, or by construction.

**Do not do**: registry data structures.

---

### M1 — Core model and lifecycle

#### T-04 — Records, registry, storage traits

| | |
|---|---|
| Milestone | M1 |
| Crates | `boa_sw_core` |
| Prerequisites | T-03 |
| Normative sections | §4.2.4, §6.3, §6.4, §6.5 |
| Diff budget | ~900 lines |

**Implement**
1. `model.rs`: `WorkerState`, `WorkerType`, `UpdateViaCache`, `RunState`, `EventType`, `WorkerRecord`, `RegistrationRecord`, `ScriptResource`, `ScriptResourceMap` exactly as §6.3/§6.4, with `serde` derives behind the feature.
2. `registry.rs`: `Registry` holding `IndexMap<(StorageKey, String /*serialized scope*/), RegistrationId>`, `HashMap<RegistrationId, RegistrationRecord>`, `HashMap<WorkerId, WorkerRecord>`, plus the lookups of §6.5 and `insert`/`remove` helpers. `match_registration` MUST pick the **longest** matching scope and skip `uninstalling` registrations.
3. `storage.rs`: `SwStorage` trait (§4.2.4) and all its data types: `PersistedRegistration`, `StorageBatch` (an ordered list of `StorageOp`: `PutRegistration`, `DeleteRegistration`, `PutWorker`, `DeleteWorker`, `PutScript`, `DeleteScriptsOf`), `CacheQuery`, `CacheOperation`, `CacheEntry`, `CacheEntryId`, `CacheName`, `CacheBatchReport`, `StorageResult`.
4. `invariants.rs`: `Registry::check_invariants(&self) -> Result<(), String>` per `R15.5.3`.

**Tests**
- Registry lookups: longest-prefix wins with three overlapping scopes; `uninstalling` skipped; `newest_worker` precedence installing → waiting → active.
- `check_invariants` detects each violation it is supposed to (one test per invariant, constructing a broken registry by hand).
- `serde` round-trip for every record type (feature on).

**Acceptance criteria**
1. `Registry` has no `pub` mutable fields; all mutation goes through methods.
2. Every invariant of `R15.5.3` has a dedicated failing-case test.
3. `cargo test -p boa_sw_core --features serde,test-util` green; coverage ≥ 92 % for the three modules.

**Do not do**: job queue, algorithms, any backend implementation.

---

#### T-05 — Job queue and the core event/command machine

| | |
|---|---|
| Milestone | M1 |
| Crates | `boa_sw_core` |
| Prerequisites | T-04 |
| Normative sections | §3.2 `AD-4`, §7.1 |
| Diff budget | ~1100 lines |

**Implement**
1. `event.rs`: `CoreEvent` and `CoreCommand` enums covering everything §3.5 and §7 need:
```rust
pub enum CoreEvent {
    ScheduleJob(JobRequest),
    ScriptFetched { job: JobId, url: Url, kind: ScriptKind, result: Result<FetchedScript, ScriptFetchError> },
    ScriptsCollected { worker: WorkerId, result: Result<ScriptResourceMap, ScriptFetchError> },
    ScriptEvaluated { worker: WorkerId, result: Result<EvaluatedInfo, ScriptEvalError> },
    EventHandled { worker: WorkerId, dispatch: DispatchId, outcome: EventOutcome },
    WorkerTerminated { worker: WorkerId },
    ClientCreated(ClientRecordCore), ClientNavigated { client: ClientId, url: Url },
    ClientUnloaded(ClientId),
    SkipWaiting { worker: WorkerId, ticket: TicketId },
    Claim { worker: WorkerId, ticket: TicketId },
    NavigationCommitted { client: ClientId, registration: RegistrationId },
    Tick { now_ms: u64, monotonic_ms: u64 },
    Shutdown,
}
pub enum CoreCommand {
    FetchScript { job: JobId, worker: Option<WorkerId>, url: Url, kind: ScriptKind, cache: CachePolicy },
    CollectScripts { worker: WorkerId },
    EvaluateScript { worker: WorkerId },
    StartWorker { worker: WorkerId },
    DispatchEvent { worker: WorkerId, dispatch: DispatchId, kind: DispatchKind },
    TerminateWorker { worker: WorkerId, reason: TerminateReason },
    Persist(StorageBatch),
    ResolveJobPromise { job: JobId, registration: Option<RegistrationId>, value: JobValue },
    RejectJobPromise { job: JobId, error: SwError },
    SettleTicket { ticket: TicketId, result: Result<(), SwError> },
    NotifyUpdateFound { registration: RegistrationId },
    NotifyWorkerStateChange { worker: WorkerId, state: WorkerState },
    NotifyControllerChange { client: ClientId, controller: Option<WorkerId> },
    ScheduleTimer { timer: TimerId, deadline_ms: u64 },
    CancelTimer { timer: TimerId },
}
```
2. `job.rs`: `Job`, `JobKind`, `JobState`, `JobStep`, `JobQueue`, `JobRequest`, `JobPromiseId`; Schedule Job with equivalence-merging (`R7.1.1`), Run Job, Finish Job (`R7.1.2`).
3. `core.rs`: `SwCore` skeleton — owns `Registry`, `IdAllocator`s, per-scope `JobQueue`s, a timer table, the client table, and the entry point `pub fn handle(&mut self, ev: CoreEvent) -> Vec<CoreCommand>` plus `pub fn check_invariants(&self)`. In this task, only `ScheduleJob`/`Tick`/`Shutdown` are handled; other events return `SwError::Internal` through a `CoreCommand`-free `todo` path marked with `unimplemented_event()` that logs and returns an empty command list (no panics).

**Tests**
- Equivalence merging: two identical `register` jobs queued back-to-back produce one job with two promises; a third, non-equivalent job stays separate; a job already running is never merged into.
- FIFO across different scopes: interleaving jobs for `/a` and `/b` produce independent queues.
- Snapshot tests (`insta`) of `Vec<CoreCommand>` for a scripted sequence.

**Acceptance criteria**
1. `SwCore::handle` is total (no panics, no `unwrap`) — enforced by a fuzz-lite test that feeds 10 000 randomly generated `CoreEvent`s and asserts no panic and `check_invariants()` after each.
2. Equivalence and FIFO tests pass; snapshots committed.
3. Public API matches the signatures above.

**Do not do**: Register/Update/Install/Activate logic (T-06), fetch or cache (M2).

---

#### T-06 — Register, Update, Soft Update

| | |
|---|---|
| Milestone | M1 |
| Crates | `boa_sw_core` |
| Prerequisites | T-05 |
| Normative sections | §6.4, §7.2, §7.3 |
| Diff budget | ~1200 lines |

**Implement**
1. `algo/register.rs`: the nine steps of §7.2 with a comment per step.
2. `algo/update.rs`: the ten steps of §7.3, byte-for-byte comparison (`R6.4.3`), `updateViaCache` → `CachePolicy` table (`R6.4.4`), script validation (`R6.4.1`, `R6.4.2`, `R6.4.6`), `last_update_check_ms` bookkeeping, Soft Update entry point `pub fn soft_update(&mut self, reg: RegistrationId, force_bypass: bool) -> Vec<CoreCommand>` with the 86 400 s rule and dedup (`R7.3.1`, `R7.3.2`).
3. Wire `CoreEvent::ScriptFetched` / `ScriptsCollected` / `ScriptEvaluated` into the job step machine.

**Tests** (state-machine level, no engine)
- register → FetchScript command with correct policy/headers metadata; script 404 → `RejectJobPromise(TypeError)` + registration removed when it was new.
- Bad MIME → `SecurityError`; redirect → `SecurityError`; oversize → `QuotaExceededError`.
- Byte-identical script → `ResolveJobPromise` and **no** `EvaluateScript` command.
- Changed script → new worker id, `CollectScripts` then `EvaluateScript`.
- `register()` on an existing registration with identical script/type/updateViaCache → runs Update on the existing registration (no second registration created).
- Soft update: `now - last_update_check <= 86_400_000` → no job; above → one job with `force_bypass_cache = true`; two consecutive soft updates → one queued job.
- Path restriction violation (via `Service-Worker-Allowed`) → `SecurityError`.

**Acceptance criteria**
1. Each of the ten Update steps is covered by at least one test (documented in `docs/traceability.md`).
2. Byte-for-byte comparison never decides on hash alone (test with a crafted equal-hash stub is not required; instead assert the code path compares slices — verified by a unit test on the comparison helper with equal hash + different bytes injected).
3. `insta` snapshots for the four principal command sequences (new registration, unchanged update, changed update, failed fetch).

**Do not do**: install/activate.

---

#### T-07 — Install, Activate, Unregister, Clear Registration, state notifications

| | |
|---|---|
| Milestone | M1 |
| Crates | `boa_sw_core` |
| Prerequisites | T-06 |
| Normative sections | §7.4 – §7.8, §7.10, §7.11 |
| Diff budget | ~1200 lines |

**Implement**
1. `algo/install.rs` (§7.4), `algo/activate.rs` (§7.5 including Try Activate and Try Clear Registration), `algo/unregister.rs` (§7.6).
2. `algo/state.rs`: Update Worker State / Update Registration State / Notify Controller Change (§7.7), each emitting the notification commands in order.
3. `algo/clients.rs`: controlled-client bookkeeping, Handle Service Worker Client Unload (`R7.10.1`), `skipWaiting` (`R7.8.1`) and `claim` (`R7.8.2`) handling of `CoreEvent::SkipWaiting`/`Claim`.
4. `algo/recovery.rs`: `SwCore::restore(persisted) -> Vec<CoreCommand>` implementing §7.11.
5. Termination policy: idle timer scheduling/cancellation, `functional_event_timeout_ms` timers, `TerminateWorker` emission (`R7.9.4`).

**Tests**
- Full happy path: register → install event Completed → worker `Installed` → no active worker → Try Activate → `Activating` → activate handled → `Activated`, with exact command snapshots.
- Install rejected → `Redundant`, registration cleared when it was new.
- Existing active worker with a controlled client: new worker stays `waiting`; after client unload → activation happens.
- `skipWaiting` on the waiting worker triggers activation and sets controllers of clients previously controlled by the old worker + `NotifyControllerChange` per client.
- `claim` sets controllers for matching uncontrolled clients only; rejects when the worker is not active.
- Unregister with a controlled client: registration no longer matched by `match_registration`, controller kept, cleared on unload.
- Recovery: crash mid-install → installing worker discarded; registration with neither waiting nor active → deleted; running recovery twice is idempotent.
- Idle timeout emits exactly one `TerminateWorker`.

**Acceptance criteria**
1. `check_invariants()` passes after **every** step of every test (helper `assert_step!`).
2. Command snapshots for the eight scenarios above are committed.
3. `R7.4.1` (promise resolved before install completes) has a dedicated test asserting command order.
4. Coverage of `algo/` ≥ 92 %.

**Do not do**: any JS binding, any storage backend.

---

#### T-08 — In-memory storage backend + storage contract test suite

| | |
|---|---|
| Milestone | M1 |
| Crates | `boa_sw_memory`, `boa_sw_core` (test-util) |
| Prerequisites | T-04 (T-07 for the integration test) |
| Normative sections | §4.2.4, §9.1 |
| Diff budget | ~900 lines |

**Implement**
1. `boa_sw_memory::MemoryStorage` implementing `SwStorage` fully, including cache storage with insertion order, `Vary` capture and quota accounting.
2. `boa_sw_core::storage::contract` (behind `test-util`): a reusable conformance suite `pub fn run_storage_contract(make: impl Fn() -> Box<dyn SwStorage>)` with ≥ 40 assertions covering: atomicity of `apply` (a batch containing a failing op writes nothing), registration round-trip, script map round-trip, cache create/open/delete/keys ordering, `cache_batch` atomicity and quota, `usage()` accuracy, and behaviour on unknown ids (`StorageError::NotFound`, never panic).
3. A fault-injecting wrapper `FailingStorage::fail_after(n)` used by `R15.5.1`.

**Acceptance criteria**
1. `run_storage_contract` passes for `MemoryStorage`.
2. Injected failures at every operation index of a 20-step scenario leave the registry consistent (`check_invariants`) and never panic.
3. `usage()` equals the sum of stored bytes in a randomized 500-operation test, within 0 bytes.

**Do not do**: SQLite.

---

### M2 — Fetch and cache core

#### T-09 — Fetch data model and Handle Fetch decision logic

| | |
|---|---|
| Milestone | M2 |
| Crates | `boa_sw_core` |
| Prerequisites | T-07 |
| Normative sections | §8 |
| Diff budget | ~900 lines |

**Implement**
1. `fetch/model.rs`: `SwRequest`, `SwResponse`, `RequestMode`, `RequestCredentials`, `RequestDestination`, `RedirectMode`, `CachePolicy`, `ResponseKind`, `NetworkError`, `Referrer`, plus constructors `SwResponse::network_error()`, `SwResponse::from_http(...)`, and `SwRequest::for_script(url, kind, policy)` which applies `R4.2.2`.
2. `fetch/handle.rs`: `SwCore::handle_fetch(&mut self, req: &SwRequest, opts) -> (FetchDecision, Vec<CoreCommand>)` implementing steps 1–7 of §8.1 (up to and including dispatch scheduling), where
```rust
pub enum FetchDecision { NotIntercepted, Dispatch { worker: WorkerId, dispatch: DispatchId }, Wait }
```
3. `fetch/validate.rs`: `pub fn validate_respond_with(req: &SwRequest, res: &SwResponse, limits: &SwLimits) -> Result<(), NetworkError>` implementing `R8.2.1`–`R8.2.6` (excluding the promise mechanics, which live in the bindings).
4. Soft-update triggering after event handling (`R7.3.1` (a)/(b)).

**Tests**
- Uncontrolled client → `NotIntercepted` with zero commands.
- `has_fetch_handler == Some(false)` → `NotIntercepted` (+ soft update command when the 24 h rule applies).
- Navigation with an `Activating` worker → `Wait`, then `Dispatch` after the activate event completes.
- Script destination and non-http schemes → `NotIntercepted`.
- `validate_respond_with` truth table: opaque×no-cors (ok), opaque×cors (error), opaqueredirect×navigate+manual (ok), status 0 basic (error), body over limit (error), used body handled by the caller (documented).

**Acceptance criteria**
1. `handle_fetch` performs no allocation on the `NotIntercepted` path — verified by a test using a counting allocator (`#[global_allocator]` in that test binary) asserting zero allocations for 1000 calls.
2. Truth-table test has ≥ 18 cases covering every branch of `validate_respond_with`.
3. Snapshots for the three interception scenarios.

**Do not do**: JS `Request`/`Response` classes.

---

#### T-10 — Cache API core algorithms

| | |
|---|---|
| Milestone | M2 |
| Crates | `boa_sw_core` |
| Prerequisites | T-08 |
| Normative sections | §9 |
| Diff budget | ~900 lines |

**Implement**
1. `cache/key.rs`: `CacheRequestKey` (method, url without fragment, header snapshot needed for `Vary`), normalization helpers.
2. `cache/query.rs`: Query Cache and Request Matches Cached Item (`R9.2.1`–`R9.2.4`), operating over `SwStorage` results.
3. `cache/batch.rs`: `pub fn plan_put(...) -> Result<Vec<CacheOperation>, SwError>` and `plan_delete`, `plan_add_all`, enforcing `R9.3.1`–`R9.3.5` and `R9.3.9`; the plan is validated before any storage call.
4. `cache/storage.rs`: `CacheStorageCore` with `open/has/delete/keys/match` (`R9.3.7`, `R9.3.8`).

**Tests**
- `Vary` matching: absent, single header, multiple headers, `*`, differing values, case-insensitive field names, whitespace-insensitive lists.
- `ignoreSearch`/`ignoreMethod`/`ignoreVary` all eight combinations.
- `put` rejection table (`R9.3.1`), one test per condition.
- `addAll` duplicate detection and all-or-nothing behaviour.
- Insertion-order preservation after `put` overwrite (the new entry takes the position of… — define: overwrite appends at the end; assert it and document).
- Quota exceeded writes nothing.

**Acceptance criteria**
1. All rejection conditions of §9.3 are individually tested (≥ 12 tests).
2. Property test: for any set of entries and any query, `query_cache` results are a subset of `keys()` and preserve relative order.
3. Differential test against a naive reference implementation over 5 000 random cases.

**Do not do**: JS classes, SQLite.

---

### M3 — Boa bindings

#### T-11 — DOM shim: `EventTarget`, `Event`, `DOMException`, `MessageEvent`, `AbortController`

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw_dom` |
| Prerequisites | T-01 |
| Normative sections | §5.5, §3.3 |
| Diff budget | ~1400 lines |

**Implement**
1. `event.rs` — `Event` class with all attributes and methods of `R5.5.2`, `EventInit` dictionary via `TryFromJs`, internal flags (`stop propagation`, `stop immediate propagation`, `canceled`, `in passive listener`, `dispatch`).
2. `event_target.rs` — `EventTarget` with the listener list, `addEventListener` options (`capture`, `once`, `passive`, `signal`), `removeEventListener`, `dispatchEvent`, and the flat dispatch algorithm of `R5.5.3`. Native structs hold `Vec<Listener>` where `Listener { ty: JsString, callback: JsObject, capture: bool, once: bool, passive: bool, removed: bool }` and are GC-traced.
3. `exception.rs` — `DOMException` with the legacy name→code table, constructor `(message, name)`, `instanceof Error`, `Symbol.toStringTag`, plus a Rust helper `pub fn throw_dom(name: &str, message: &str, ctx: &mut Context) -> JsError`.
4. `message_event.rs`, `error_event.rs` — per HTML, with `initMessageEvent` omitted.
5. `abort.rs` — `AbortController`/`AbortSignal` (`abort(reason)`, `aborted`, `reason`, `throwIfAborted`, `AbortSignal.abort()`, `AbortSignal.timeout(ms)` returning a signal driven by the runtime clock — may be `NotSupportedError` when no timer source is available).
6. `register.rs` — `pub fn register(realm: Option<Realm>, ctx: &mut Context) -> JsResult<()>` registering all of the above, and `pub fn register_subclass_prototype(...)` used by other crates to build classes that inherit from `EventTarget`.
7. `helpers.rs` — `pub fn make_event_handler_accessor(target: &JsObject, name: &str, ctx: &mut Context) -> JsResult<()>` implementing `R5.0.6` `onX` semantics on any `EventTarget`.

**Tests** (JS-driven, run through a `Context`)
- Listener ordering, duplicate suppression, `once`, removal during dispatch, `stopImmediatePropagation`.
- `dispatchEvent` return value with `cancelable`/`preventDefault`.
- `onX` handler: set → dispatch → fires; set to `null` → no fire; set twice keeps position.
- `DOMException` name/code table for all 25 legacy names; `instanceof DOMException` and `instanceof Error`.
- `AbortSignal` propagation into `addEventListener(..., { signal })` removal.
- Subclass prototype chain: a class registered through `register_subclass_prototype` satisfies `x instanceof EventTarget`.

**Acceptance criteria**
1. A JS test file `tests/dom_shim.js` (≥ 60 assertions) executed by the Rust test harness passes entirely.
2. `boa_sw_dom` compiles with `--no-default-features`.
3. No `JsObject` is retained after its `EventTarget` is dropped (leak test with `WeakGc`).

**Do not do**: Service-Worker-specific events.

---

#### T-12 — Fetch classes: `Headers`, `Request`, `Response`, Body mixin, `fetch()`

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw_fetch` |
| Prerequisites | T-09, T-11 |
| Normative sections | §5.4, §8, `AD-7`, `AD-8` |
| Diff budget | ~1800 lines |

**Implement**
1. `headers.rs` — guard model, name/value validation, `append`/`set`/`get`/`has`/`delete`/`getSetCookie`, sorted-combined iteration, `forEach`, `Symbol.iterator`, construction from `Headers`/record/array-of-pairs.
2. `request.rs` — `Request` class over `SwRequest`, all members of `R5.4.2`, `clone()`, body extraction from `string`, `ArrayBuffer`, `TypedArray`, `URLSearchParams` (when available) and `null`.
3. `response.rs` — `Response` class over `SwResponse`, all members of `R5.4.3`, statics `error`/`redirect`/`json`, `clone()`.
4. `body.rs` — Body mixin (`R5.4.4`) shared by `Request` and `Response`, with the disturbed/locked flags.
5. `fetch.rs` — `fetch(input, init)` returning a promise, driven by `HttpClient` through the runtime's ticket mechanism; `AbortSignal` support (`R5.4.7`); `max_body_bytes` enforcement.
6. `convert.rs` — conversions `SwRequest ⇄ Request`, `SwResponse ⇄ Response`, and (feature `runtime-interop`) adapters to `boa_runtime::fetch::{JsRequest, JsResponse}` plus `impl HttpClient for RuntimeFetcherClient<T: Fetcher>`.
7. Global-conflict detection per `AD-8`.

**Tests**
- Headers guard behaviour (immutable response headers of an opaque response reject mutation with `TypeError`).
- Header iteration order (sorted by name, values combined with `", "`).
- `Request` clone with used body → `TypeError`; `Request` with `GET` + body → `TypeError`.
- `Response.error()` fields; `Response.redirect` status validation (`RangeError` for non-redirect statuses); `Response.json` content type.
- Body double-consume → `TypeError`; `text()`/`json()`/`arrayBuffer()`/`bytes()` round-trips including UTF-8 with a BOM and invalid UTF-8 (replacement chars).
- `fetch()` success, network error, abort, oversize body.

**Acceptance criteria**
1. JS test file `tests/fetch_classes.js` (≥ 120 assertions) passes.
2. Every rejection condition of §5.4 has a test.
3. With `runtime-interop`, a round-trip `SwRequest → JsRequest → SwRequest` preserves method, url, headers and body (property test, 1 000 cases).

---

#### T-13 — `SwRuntime`: installation, realms, scheduler, command execution

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-07, T-08, T-11 |
| Normative sections | §3.4, §4.1, §4.3, §11.1, §11.2 |
| Diff budget | ~1700 lines |

**Implement**
1. `runtime.rs` — `SwRuntime` struct: `Rc<Self>` handle, `RefCell<SwCore>`, host handles, ticket tables, timer heap, `in_pump` flag, per-worker `WorkerRuntime { realm: Realm, global: JsObject, identity: IdentityMaps, elps: … }`, and the module documentation quoting `R3.4.1`–`R3.4.6` verbatim.
2. `install.rs` — `SwRuntime::install` (`R4.1.1`–`R4.1.3`) and `SwRuntime::get`.
3. `realm_util.rs` — `with_realm` (`R3.4.3`) and realm creation/teardown for workers.
4. `pump.rs` — the six-phase `pump` (`R3.4.6`), `PumpOutcome`, `max_pump_iterations` budget, timer expiry.
5. `commands.rs` — the executor translating each `CoreCommand` into runtime actions; commands not yet implemented (dispatch, evaluate) return `SwError::NotSupported` through the observer and are covered in later tasks. Exhaustive `match`, no wildcard arm.
6. `tickets.rs` — `FetchTicketId`, `FetchCompleter`, `HostTicket<T>`, the MPSC drain, and late/duplicate-completion handling (`R4.2.3`, `R4.2.10`).
7. Host trait re-exports and a `test_util` module with `FakeHttp`, `FakeClients`, `FakeClock`, `FakeStorage` builders used by all later tasks.

**Tests**
- `install` twice → `AlreadyInstalled`; `install` restores persisted registrations and applies recovery.
- `pump` re-entrancy → `Reentrant`.
- Phase ordering: a scripted scenario asserts the observer event sequence matches the phase order.
- Budget exhaustion returns `PumpOutcome::Budget` and does not lose work.
- A completion arriving after `cancel` is dropped without side effects.
- Realm creation/teardown 100× leaves no growth in the identity maps.

**Acceptance criteria**
1. `commands.rs` matches every `CoreCommand` variant explicitly (compile-time exhaustiveness).
2. All tests above pass; the phase-ordering test uses `insta` snapshots.
3. `cargo clippy -p boa_sw --all-features -- -D warnings` clean.

**Do not do**: any JS class registration beyond what `install` needs for `navigator.serviceWorker` to exist as a stub object.

---

#### T-14 — Worker startup: `ServiceWorkerGlobalScope`, script evaluation, `importScripts`

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-13, T-12 |
| Normative sections | §5.2.1, §7.9, §11.4 |
| Diff budget | ~1400 lines |

**Implement**
1. `worker/global.rs` — creation of a SW realm, `ServiceWorkerGlobalScope` prototype chain (`R5.2.1`), the exact global surface of `R5.2.2`, `self`, `skipWaiting`, `registration`/`serviceWorker` `[SameObject]` accessors (returning objects created in T-16 — until then, stubs guarded by a feature-independent `cfg(test)` fallback are **not** allowed: implement the accessors to call into an internal `ObjectFactory` trait that T-16 fills in).
2. `worker/start.rs` — Run Service Worker (`R7.9.1`), including loading scripts from the script resource map, evaluating with the script URL as source path, capturing `has_fetch_handler` and `handled_event_types` (`R8.1.2`), and reporting `CoreEvent::ScriptEvaluated`.
3. `worker/imports.rs` — `importScripts` per `R11.4.2` and the collect-then-evaluate protocol of `R11.4.4` (classic case), bounded by `max_imported_scripts`.
4. `worker/terminate.rs` — Terminate Service Worker (`R7.9.3`): realm drop, identity-map clearing, ELP invalidation, `run_state` reset.

**Tests**
- Global surface test: a JS snippet enumerates `Object.getOwnPropertyNames(self)` and compares against the expected list of `R5.2.2` (exact set equality).
- `self.registration`, `self.serviceWorker` identity stability.
- Script throwing at top level → `ScriptEvaluated(Err)` and no realm leak.
- `has_fetch_handler` detection for: `addEventListener('fetch', …)`, `onfetch = …`, neither.
- `importScripts` of a cached script, of a new script (triggering the collect protocol), of an unknown script after evaluation → `NetworkError`, and exceeding the import limit.
- Start → terminate → start again works without re-fetching (assert `FakeHttp` request count).

**Acceptance criteria**
1. The global-surface set-equality test passes exactly (no extra, no missing names).
2. Terminate/restart cycle test shows zero additional HTTP requests.
3. Realm leak test: after 100 start/terminate cycles, `WeakGc` handles to the globals are all dead.

---

#### T-15 — Event dispatch pipeline and extended lifetime promises

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-14 |
| Normative sections | §5.2.2, §7.4, §7.5, §11.3 |
| Diff budget | ~1200 lines |

**Implement**
1. `events/extendable.rs` — `ExtendableEvent` class (constructor, `waitUntil`) with the "active" window logic of `R11.3.2` and `InvalidStateError` otherwise.
2. `events/dispatch.rs` — `DispatchKind` → event object construction → dispatch on the worker global → ELP collection → outcome aggregation (`R11.3.3`) → `CoreEvent::EventHandled`. Includes timeout timers and the `Terminated` outcome.
3. `events/elp.rs` — ELP registration via `JsPromise::then` with native reactions, counter maintenance on `WorkerRecord::pending_events` (through a core event, never by mutating core state directly), and invalidation on termination (`R11.3.5`).
4. Wiring of `CoreCommand::DispatchEvent` for `install` and `activate`.

**Tests**
- `install` handler with `waitUntil(Promise.resolve())` → `Completed`; with a rejection → `Rejected` and the worker becomes `Redundant`.
- `waitUntil` called after the window closed → `InvalidStateError`.
- Chained `waitUntil` inside a `waitUntil` reaction (still active) extends the window.
- Timeout: a never-settling promise produces `TimedOut` after the configured time on a `FakeClock`.
- Termination mid-event produces `Terminated` and settles nothing else.
- Counter invariant: `pending_events` returns to 0 in every scenario.

**Acceptance criteria**
1. End-to-end: `register()` on a script with `install`+`activate` handlers reaches state `activated` and the registration promise resolves before the install event completes (`R7.4.1`).
2. Every outcome variant (`Completed`, `Rejected`, `TimedOut`, `Terminated`) has a test.
3. `pending_events == 0` asserted after each test via `check_invariants`.

---

#### T-16 — Client-context API: container, registration, worker objects, navigation preload

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-15 |
| Normative sections | §5.1, §7.7, §3.3 |
| Diff budget | ~1600 lines |

**Implement**
1. `api/container.rs` — `ServiceWorkerContainer` with `register`, `getRegistration`, `getRegistrations`, `startMessages`, `ready`, `controller`, `oncontrollerchange`, `onmessage`, `onmessageerror`; installation on `navigator` (`R5.1.1`).
2. `api/registration.rs` — `ServiceWorkerRegistration` with all members of §5.1.2, per-realm identity map (`R3.3.4`).
3. `api/worker.rs` — `ServiceWorker` class (§5.1.3) with `postMessage` delegating to T-20 (until then, `postMessage` returns `NotSupportedError` — this is the **only** permitted temporary stub, and T-20 must remove it).
4. `api/navigation_preload.rs` — `NavigationPreloadManager` (§5.1.4), persisted flags.
5. `notify.rs` — handling of `NotifyUpdateFound` / `NotifyWorkerStateChange` / `NotifyControllerChange` (`R7.7.4`): queued as tasks, delivered in order, fired on all live objects in all realms.
6. `ObjectFactory` implementation filling the hooks left by T-14.

**Tests**
- `register()` argument validation table (≥ 12 cases mapping to `TypeError`/`SecurityError` per §5.1.2).
- Identity: `reg === await navigator.serviceWorker.getRegistration()` twice; `reg.installing === reg.installing`.
- `statechange` fires in order `installing → installed → activating → activated` on the same `ServiceWorker` object.
- `updatefound` fires once per new installing worker, on registration objects obtained before and after.
- `ready` resolves after activation, and immediately when an active worker already matches.
- `controller` is `null` before the client is controlled and becomes non-null after `claim()`.
- `navigationPreload.enable()/getState()` round-trip and persistence across a simulated restart.

**Acceptance criteria**
1. JS test file `tests/client_api.js` (≥ 90 assertions) passes.
2. Identity maps release objects when realms are dropped (leak test).
3. All notification orderings verified with `insta` snapshots of the observer log.

---

#### T-17 — `Clients`, `Client`, `WindowClient`

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-16 |
| Normative sections | §5.2.3, §4.2.3, §7.8.2 |
| Diff budget | ~1000 lines |

**Implement**
1. `api/clients.rs` — `Clients` with `get`, `matchAll` (including the ordering of `R5.2.11`), `openWindow`, `claim`.
2. `api/client.rs` — `Client` and `WindowClient` classes; `focus()`/`navigate()` wired through `ClientHost` tickets with promise settlement on ticket completion.
3. Client bookkeeping in the runtime: `on_client_created/navigated/unloaded/focus_changed/visibility_changed` forwarding to core events.

**Tests**
- `matchAll` ordering with three window clients of different focus order plus one worker client; `includeUncontrolled` true/false; `type` filter values.
- `get` with an unknown id → `undefined`; with a cross-origin client → `undefined`.
- `claim()` sets controllers and fires `controllerchange` exactly once per client.
- `openWindow` resolving with a client, resolving with `null`, and rejecting with `InvalidAccessError`.
- `navigate()` on an uncontrolled client → `TypeError`/`InvalidAccessError` per §5.2.15.
- A dropped `HostTicket` rejects the promise with `AbortError`.

**Acceptance criteria**
1. JS test file `tests/clients_api.js` (≥ 50 assertions) passes.
2. The ordering test is deterministic across 100 repeated runs.

---

#### T-18 — `FetchEvent`, interception integration, navigation preload

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-15, T-12, T-09 |
| Normative sections | §4.3, §8 |
| Diff budget | ~1300 lines |

**Implement**
1. `events/fetch_event.rs` — `FetchEvent` class with all members of `R5.2.8`, `respondWith` (`R5.2.7`), `handled` promise.
2. `fetch/interception.rs` — `SwRuntime::handle_fetch` and `FetchInterception`/`FetchInterceptionState` (§4.3), including the `Wait` state for activating workers, timeouts (`R4.3.2`), cancellation, and response validation through `validate_respond_with`.
3. `fetch/preload.rs` — navigation preload (`R8.3.1`–`R8.3.3`) behind the feature.
4. Controller assignment on navigation commit (step 11 of §8.1) and soft-update triggering (step 12).

**Tests**
- Uncontrolled request → `NotIntercepted` synchronously, no pump needed.
- `respondWith(new Response('hi'))` → `Responded` with the body.
- No `respondWith` → `FallbackToNetwork`.
- Handler throws → `Failed`; `respondWith` with a rejected promise → `Failed`; with a non-Response → `Failed` and `handled` rejects with `TypeError`.
- Opaque/opaqueredirect/status-0 validation cases.
- Timeout → `Failed(Timeout)` and ELPs rejected.
- Navigation: after `Responded`, the resulting client becomes controlled and `controllerchange` does **not** fire (it was never controlled by another worker).
- Preload: enabled → `preloadResponse` resolves with the response and the extra header was sent; unused preload is cancelled.
- `fetch()` inside the worker is never re-intercepted (`R5.4.6`).

**Acceptance criteria**
1. JS+Rust test file pair covering all ten scenarios above passes.
2. `handle_fetch` for an uncontrolled request performs zero allocations (re-uses the T-09 counting-allocator test at runtime level).
3. `handled` settles exactly once in every scenario (asserted with a counter).

---

#### T-19 — `Cache` and `CacheStorage` JS classes

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-10, T-12 |
| Normative sections | §5.3, §9 |
| Diff budget | ~1100 lines |

**Implement**
1. `api/cache.rs` — `Cache` class: `match`, `matchAll`, `add`, `addAll`, `put`, `delete`, `keys`, all mapped onto `boa_sw_core::cache` plans and `SwStorage`.
2. `api/cache_storage.rs` — `CacheStorage`: `open`, `has`, `delete`, `keys`, `match`; registration of `caches` in both realms per `SwConfig`.
3. Conversion of JS `Request`/`Response` to/from core cache entries, including body cloning rules (`R5.3.4`).
4. `InvalidStateError` behaviour for a `Cache` whose backing cache was deleted (`R9.3.8`).

**Tests**
- Full CRUD round-trip in JS; `keys()` ordering; `match` with all query options.
- `put` rejection cases (§9.3.1) — one JS test each.
- `addAll` atomicity: a failing URL leaves the cache untouched.
- Response identity: `(await cache.match(r)) !== (await cache.match(r))` and both bodies readable.
- Quota exceeded surfaces as `QuotaExceededError`.
- Cache usable from both the client realm and the SW realm on the same data.

**Acceptance criteria**
1. JS test file `tests/cache_api.js` (≥ 100 assertions) passes on the memory backend.
2. Every §9.3 rejection condition is covered.
3. No `Response` body is copied more than once per `match` (asserted by an `Rc::strong_count` test in Rust).

---

#### T-20 — Messaging: `postMessage`, message queues, `ExtendableMessageEvent`

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-16, T-17 |
| Normative sections | §5.1.3, §5.2.2, §10 |
| Diff budget | ~1100 lines |

**Implement**
1. `messaging/serialize.rs` — structured clone through `boa_wintertc::store::JsValueStore` (feature `runtime-interop`) and the fallback serializer (`R10.3.2`), with `DataCloneError` thrown synchronously.
2. `messaging/queues.rs` — per-client and per-worker message queues, the enable-on-first-listener rule (`R10.1.3`), FIFO guarantees, persistence-free but restart-safe ordering (`R10.1.4`).
3. `events/message_event.rs` — `ExtendableMessageEvent` (`R5.2.9`) and the client-side `MessageEvent` path (`R10.2.2`), `messageerror` on deserialization failure.
4. Removal of the temporary `ServiceWorker.postMessage` stub from T-16; `Client.postMessage` wiring for local and foreign clients (`ClientHost::deliver_message` + `deliver_message_to_local_client`).

**Tests**
- Page → worker → page round-trip with an object graph containing a cycle, a `Map`, a `Set`, a `Date`, a `TypedArray`.
- Message to a stopped worker starts it and is delivered after evaluation.
- Messages queued before `onmessage` is set are delivered in order once set; `startMessages()` has the same effect.
- `postMessage` to a `redundant` worker → `InvalidStateError`.
- Unserializable value (a function) → `DataCloneError` thrown synchronously.
- `source` is a `Client` on the worker side, a `ServiceWorker` on the page side.
- Transferred `ArrayBuffer` is detached in the sender.
- A `MessagePort` in the transfer list is rejected with `DataCloneError` **until `T-27` lands**; `T-27` replaces this test with the positive one.

**Acceptance criteria**
1. JS test file `tests/messaging.js` (≥ 60 assertions) passes with and without `runtime-interop`.
2. Ordering is preserved across a worker termination in the middle of a message burst.
3. No temporary stub from T-16 remains (grep for `NotSupportedError` in `api/worker.rs` returns nothing).

---

#### T-21 — Module service workers (`type: "module"`)

| | |
|---|---|
| Milestone | M3 |
| Crates | `boa_sw` |
| Prerequisites | T-14 |
| Normative sections | §11.4.3, §11.4.4 |
| Diff budget | ~800 lines |

**Implement**
1. `worker/module.rs` — a `ModuleLoader` serving only from the worker's script resource map; module-graph collection through `HttpClient` (static import scan + recursive fetch), validation of every module response as a script resource, linking and evaluation.
2. `importScripts` throws `TypeError` in module workers (`R5.2.5b`).
3. `updateViaCache` applies to module imports exactly as to classic imports.
4. Feature gate `module-workers`; without it, `{ type: "module" }` registration rejects with `NotSupportedError`.

**Tests**
- A worker importing two modules installs and activates; `FakeHttp` shows exactly three requests.
- A module with a syntax error fails the job with the parse message.
- A missing import (404) fails the job with `TypeError`.
- Byte-for-byte update comparison covers imported modules (changing only an import triggers a new worker).
- Without the feature, registration rejects with `NotSupportedError`.

**Acceptance criteria**
1. All five tests pass with `--features module-workers` and the last one passes with `--no-default-features`.
2. No network access happens during linking (assert `FakeHttp` request count is unchanged during the link/evaluate phase).

---

#### T-27 — `MessageChannel` / `MessagePort`

| | |
|---|---|
| Milestone | M3 (may run after T-20; blocks release) |
| Crates | `boa_sw`, `boa_sw_dom` |
| Prerequisites | T-20 |
| Normative sections | §5.5 (`R5.5.7`), §10.3, §10.4 |
| Diff budget | ~1100 lines |

**Implement**
1. `messaging/port.rs` — `MessagePort` class (an `EventTarget`): `postMessage(message, transfer | options)`, `start()`, `close()`, `onmessage`, `onmessageerror`; per-port message queue with the enabling rules of `R5.5.7`.
2. `messaging/channel.rs` — `MessageChannel` constructor producing two entangled ports (`port1`, `port2`).
3. `messaging/port_table.rs` — the realm-independent `PortPair` table (`R10.4.1`), delivery during `pump` (`R10.4.2`), detach-on-transfer (`R10.4.3`), close/realm-teardown semantics (`R10.4.5`, `R10.4.6`).
4. Transfer integration: ports in the transfer list of `ServiceWorker.postMessage`, `Client.postMessage` and `port.postMessage`; the `is_transferable` check of `R10.4.7`; `ExtendableMessageEvent.ports` populated for real.
5. Worker keep-alive on port messages (`R10.4.4`).
6. Registration of both classes in the client realm and every SW realm; removal of the negative test added in T-20.

**Tests**
- Request/response round-trip: page creates a channel, transfers `port2` to the worker with `postMessage(msg, [port2])`, the worker replies on `event.ports[0]`, the page receives it on `port1.onmessage`.
- The same pattern initiated by the worker towards a `Client`.
- A port transferred while it has undelivered queued messages: the messages arrive in the destination realm, in order.
- `postMessage` on a transferred (detached) port → `InvalidStateError`.
- Messages sent before `start()`/`onmessage` are queued and delivered on enabling; `addEventListener("message", …)` alone does **not** enable the port.
- A message to a port whose twin lives in a stopped worker starts that worker and is delivered after evaluation.
- `close()` on either side stops delivery both ways; no error is thrown by later sends.
- Worker termination with a transferred port: the twin sees the port as closed, and a `WeakGc` leak test shows the port object is collected.
- With and without `runtime-interop`, transferring a port behaves identically (`R10.4.7`).

**Acceptance criteria**
1. JS test file `tests/message_ports.js` (≥ 50 assertions) passes with `--all-features` and with `--no-default-features --features memory-backend`.
2. The WPT files `postmessage.https.html` and `extendable-message-event.https.html` are 100 % green (they are on the priority list of `R15.3.3`).
3. Port-table size returns to zero after every test (asserted by a runtime introspection helper), i.e. no leaked entangled pairs.
4. The T-20 negative test for port transfer is removed, not skipped.

---

### M4 — Persistence and host integration

#### T-22 — SQLite backend

| | |
|---|---|
| Milestone | M4 |
| Crates | `boa_sw_sqlite` |
| Prerequisites | T-08 |
| Normative sections | §4.2.4, §13.2, §13.6, Appendix C |
| Diff budget | ~1600 lines |

**Implement**
1. The schema of Appendix C with `PRAGMA journal_mode=WAL`, `foreign_keys=ON`, `synchronous=NORMAL` (configurable), a `schema_version` table and a migration runner.
2. `SqliteStorage` implementing `SwStorage`: prepared-statement cache, `apply` and `cache_batch` inside a single transaction, `usage()` maintained by triggers or by an explicit counter row (choose the counter row; document in an ADR).
3. Blob storage for bodies and script bytes with a size threshold above which the body goes to a side table (keeps the main table hot); default threshold 64 KiB.
4. Advisory locking so a second process fails to open (`R13.6`).
5. `SqliteStorage::open(path)`, `open_in_memory()`, `vacuum()`, `checkpoint()`.

**Tests**
- The full `run_storage_contract` suite from T-08 passes.
- Differential test vs `MemoryStorage`: 10 000 randomized operations, identical observable results.
- Restart test: write, drop, reopen, all data present.
- Crash test: kill between batches (drop the connection mid-transaction via a fault-injecting VFS shim or by aborting the transaction) → recovery leaves a consistent registry.
- Second-process open fails with `SwError::Storage`.
- `EXPLAIN QUERY PLAN` assertions for the three hot queries (registration lookup by key, cache query by url+method, cache keys by cache id) show index usage, not a scan.

**Acceptance criteria**
1. Storage contract + differential + restart + crash suites all green.
2. Query-plan test asserts `SEARCH` (not `SCAN`) for the three hot queries.
3. `usage()` matches a full recomputation after 1 000 random operations.

---

#### T-23 — Host integration, recovery, shutdown, demo host

| | |
|---|---|
| Milestone | M4 |
| Crates | `boa_sw`, `examples/offline_shell` |
| Prerequisites | T-18, T-19, T-20, T-22 |
| Normative sections | §4.1, §4.3, §7.10, §7.11, Appendix D |
| Diff budget | ~1200 lines |

**Implement**
1. `SwRuntime::shutdown` (`R4.3.3`) and `Handle User Agent Shutdown` (`R7.10.2`).
2. Restart recovery end-to-end (`R7.11.x`) with the SQLite backend.
3. `examples/offline_shell`: a CLI host with a tiny in-process HTTP server serving a static site plus a service worker, a fake window client, and commands `register`, `fetch <path>`, `update`, `unregister`, `restart`, `dump`. It MUST run the Appendix D acceptance scenario end-to-end, on the SQLite backend by default (`--backend memory` switches).
3a. `host_contract.rs` — a **host-contract validator** the runtime can be built with (`SwConfig::strict_host_contract`, default on in debug): it checks `R6.1.2a` at runtime (a navigation without `reserved_client_id`; an id reused after unload; `on_client_created` never called for a committed reserved id; a `replaces_client_id` that names an unknown client) and reports each violation through `SwObserver` with an actionable message. Include the same checks as a reusable test helper so host authors can run them against their own implementation.
4. `docs/host-integration.md`: how to implement each host trait, with the demo host as the reference.

**Tests**
- Appendix D scenario as an automated integration test on both backends.
- Restart: register → activate → shutdown → re-install → the worker still controls the client after re-navigation, caches intact.
- Shutdown with an in-flight fetch interception rejects it cleanly.

**Acceptance criteria**
1. `cargo run -p offline_shell -- demo` runs the whole Appendix D scenario and prints `OK` for every step, on both `memory` and `sqlite` backends.
2. The restart test passes 20 consecutive times.
3. `docs/host-integration.md` covers all five host traits with compiling code samples (`cargo test --doc`), and contains a dedicated section on the client-identity contract (`R6.1.2a`) with a correct and an incorrect example.
4. The host-contract validator detects all four violation classes of item 3a (one test each) and the demo host triggers none of them.

---

### M5 — Conformance and release

#### T-24 — WPT harness and expectations

| | |
|---|---|
| Milestone | M5 |
| Crates | `boa_sw_wpt` |
| Prerequisites | T-23 |
| Normative sections | §15.3 |
| Diff budget | ~1400 lines |

**Implement**
1. A `testharness.js` shim, a static file server over the WPT checkout, and a driver that creates a client realm per test file, runs the test, and collects subtest results.
2. `expectations.json` with per-file/per-subtest expected status and a reason for every exclusion.
3. `cargo run -p boa_sw_wpt -- --wpt <path> [--filter <glob>] [--update-expectations]`, printing a summary and exiting non-zero on unexpected results.
4. `docs/wpt.md` explaining how to run it and how to triage.

**Acceptance criteria**
1. The runner executes the whole `service-workers/service-worker/` directory without crashing or hanging (per-test timeout enforced).
2. ≥ 70 % of runnable subtests PASS; every FAIL/SKIP has a reason string; `crates/boa_sw_wpt/priority.txt` exists and the runner fails when any file in it has a non-PASS subtest (at `T-24` the priority list may still contain failures — they are the punch list for `T-25`/`T-27`; the gate is enabled at release).
3. Re-running with `--update-expectations` produces no diff when nothing changed (idempotent).

---

#### T-25 — Hardening: fuzz, crash consistency, performance, leaks

| | |
|---|---|
| Milestone | M5 |
| Crates | all |
| Prerequisites | T-24 |
| Normative sections | §14, §15.4, §15.5 |
| Diff budget | ~1200 lines |

**Implement**
1. Four fuzz targets (`script_response_validation`, `header_parsing`, `cache_query`, `structured_clone_fallback`).
2. The determinism/golden-log suite of `R15.4.1`.
3. The crash-consistency suite of `R15.5.2` (200 randomized runs).
4. `criterion` benchmarks for every row of §14.1 plus a `benchmark-gate` script comparing against a committed baseline.
5. The leak/GC suite of `R14.2.1`.

**Acceptance criteria**
1. Each fuzz target runs 30 minutes with no crash (CI runs 2 minutes per target; the long run is documented in the handoff).
2. All golden logs are byte-identical across two consecutive runs and across Linux/macOS/Windows.
3. Benchmarks meet §14.1 targets or every deviation is justified in writing with a profile attached.
4. The leak suite shows no growth beyond the documented constant.

---

#### T-26 — Documentation, examples, release 0.1.0

| | |
|---|---|
| Milestone | M5 |
| Crates | all |
| Prerequisites | T-25 |
| Normative sections | §1.4, §15.2, §17 |
| Diff budget | ~900 lines |

**Implement**
1. Crate-level rustdoc for all seven crates with runnable examples; `#![warn(missing_docs)]` clean.
2. `README.md` (root): what it is, quick start, feature matrix, supported/unsupported table.
3. `docs/compat.md` finalized (every §1.4 item plus every deviation discovered during implementation, each with the observable behaviour).
4. `docs/traceability.md` complete: every `Rx.y.z` → test name; every Spec algorithm → module (Appendix E).
5. `CHANGELOG.md`, licence files, `cargo publish --dry-run` for the publishable crates.

**Acceptance criteria**
1. `cargo doc --workspace --all-features` with zero warnings; all doc examples compile and run (`cargo test --doc`).
2. `docs/traceability.md` has no empty cells; a script `scripts/check_traceability.rs` verifies that every `R`-id present in the TS appears in the file and that every referenced test exists.
3. `cargo publish --dry-run` succeeds for `boa_sw_core`, `boa_sw_dom`, `boa_sw_fetch`, `boa_sw`, `boa_sw_memory`, `boa_sw_sqlite`.
4. All criteria of §17.1 hold.

---

## 17. Overall acceptance, risks, open questions

### 17.1. Acceptance criteria for the whole work

The work is accepted when **all** of the following hold simultaneously:

1. **Functionality.** Every interface of Appendix A and every algorithm of §7–§10 is implemented; the only deviations are those listed in §1.4 and mirrored in `docs/compat.md`.
2. **Conformance.** WPT `service-workers/service-worker/**`: ≥ **85 %** of runnable subtests PASS on both the SQLite (default) and the memory backend; **every file in `crates/boa_sw_wpt/priority.txt` is 100 % green with no expectation entries** (`R15.3.3`); zero CRASH/TIMEOUT; every non-PASS has a reason in `expectations.json`.
3. **Platform.** Builds and tests are green on the three CI OSes with `rustc 1.91.0`, with no warnings; `boa_sw_core` builds for `wasm32-unknown-unknown`.
4. **Code quality.** `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`, `cargo deny check` clean; zero `unsafe`; zero `unwrap`/`expect` outside tests.
5. **Coverage.** ≥ 90 % `boa_sw_core`; ≥ 80 % `boa_sw`, `boa_sw_fetch`, `boa_sw_dom`; `docs/traceability.md` complete and verified by `scripts/check_traceability.rs`.
6. **Determinism.** Golden-log suites byte-identical across runs and platforms (`R15.4.2`).
7. **Robustness.** 200 crash-consistency runs, 10 000 differential storage operations, 4 fuzz targets × 30 min — no failures, no panics.
8. **Performance.** §14.1 targets met or each deviation justified with a profile.
9. **Memory.** `R14.2.1` leak suite green.
10. **Documentation.** `cargo doc` warning-free; `README.md`, `docs/compat.md`, `docs/host-integration.md`, `docs/wpt.md` complete.
11. **Demonstration.** The Appendix D scenario runs in `examples/offline_shell` on both backends, including a process restart, and the site remains offline-capable afterwards.

### 17.2. Risks

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | Boa 0.22 lacks an interrupt facility → a runaway worker cannot be killed | Medium | `AD-10` documents the limitation; `set_interrupt_hook` is the extension point; escalate to upstream Boa if a host needs it |
| R2 | `importScripts` is synchronous in the Spec but the runtime cannot block on the network | High | `R11.4.4` collect-then-evaluate with a bounded retry loop; observable deviation documented; `pre_declared_imports` escape hatch |
| R3 | No Streams → `respondWith` of a streamed response is impossible; large media cannot be intercepted efficiently | Medium | `AD-7` buffering with a size cap; hosts route large responses around the worker; Streams are a v1.1 item |
| R4 | WPT for service workers leans heavily on `document`, iframes and real navigations | High | The harness models one window client and a fake navigation; excluded files enumerated with reasons; target set at 85 % rather than 95 % |
| R5 | Realm-per-worker shares one GC heap and one thread with the page; a slow worker blocks the page | Medium | Documented; the architecture keeps every boundary message-shaped so thread-per-worker is a runtime-only change in v1.1 |
| R6 | `has_fetch_handler` persistence can go stale if a script is edited without a version bump | Low | It is recomputed on every evaluation and re-persisted; byte-for-byte comparison guarantees the script matches the flag |
| R7 | Cache API semantics for deleted caches deviate from browsers (`R9.3.8`) | Low | Documented in `docs/compat.md`; revisit if WPT requires it |
| R8 | Structured-clone fallback (without `runtime-interop`) may diverge from `JsValueStore` | Medium | Differential test between the two implementations over generated value graphs (T-20); for `MessagePort` transfer specifically, `R10.4.7` forces an explicit check of `boa_wintertc`'s `is_transferable` gate, with this crate's own transfer path as the fallback |
| R9 | Spec is a living document; algorithm text may shift during development | Low | `docs/SPEC_REVISION.md` pins the revision; changes are a separate change request |

### 17.3. Customer decisions (settled 2026-09-04)

These questions were open when the TS was drafted; they are now answered and the answers are
normative. Reopening any of them is a change request, not an implementer's choice.

| # | Question | Decision | Where it lands |
|---|---|---|---|
| `D-1` | Default storage backend for docs, examples and CI | **SQLite.** `sqlite-backend` is in `default-features`; `boa_sw_memory` remains for tests and for hosts that opt out (`--no-default-features`). A host that selects the memory backend is warned once at `install` | §2.4, `R2.4.2`, T-22, T-23, §17.1 |
| `D-2` | Is `caches` required in the client realm | **Yes, by default**, per the Spec's `WindowOrWorkerGlobalScope` mixin. `SwConfig::caches_in_client_realm = true`; turning it off is the host's documented deviation | `R5.3.1`, T-19 |
| `D-3` | `MessageChannel`/`MessagePort` in v1.0 | **Yes, full support**, including port transfer in both directions and a populated `ExtendableMessageEvent.ports`. Rationale: the `postMessage(msg, [port2])` + `port1.onmessage` reply pattern used by real service-worker libraries fails *silently* without it, which breaks `R1.1.1` | §1.3 item 2a, `R5.5.7`, §10.4, **new task T-27**, +2 person-weeks |
| `D-4` | WPT contract | **Two-part:** ≥ 85 % of runnable subtests at release **and** every file in `crates/boa_sw_wpt/priority.txt` 100 % green with no expectation entries. A bare percentage is gameable by filtering; the file list is not. Streams/`Blob`/iframe-client work (≈ +6 person-weeks for ≥ 92 %) is **not** in v1.0 | `R15.3.3`, §17.1 item 2, T-24 |
| `D-5` | `ClientId` lifecycle across navigations | **The host owns it, per the Spec:** a `ClientId` identifies a *document*, not a tab. The host allocates a fresh id per navigation and supplies `reserved_client_id`/`replaces_client_id`; the runtime never generates ids. Enforced at runtime by the host-contract validator | `R6.1.2a`, `R8.4.1`, T-23 |

No open questions remain. New ones go to `docs/QUESTIONS.md`.

---

## 18. Appendices

### Appendix A. WebIDL — normative implementation scope

```webidl
[SecureContext, Exposed=(Window,Worker)]
interface ServiceWorker : EventTarget {
  readonly attribute USVString scriptURL;
  readonly attribute ServiceWorkerState state;
  undefined postMessage(any message, sequence<object> transfer);
  undefined postMessage(any message, optional StructuredSerializeOptions options = {});
  attribute EventHandler onstatechange;
};
enum ServiceWorkerState { "parsed", "installing", "installed", "activating", "activated", "redundant" };

[SecureContext, Exposed=(Window,Worker)]
interface ServiceWorkerRegistration : EventTarget {
  readonly attribute ServiceWorker? installing;
  readonly attribute ServiceWorker? waiting;
  readonly attribute ServiceWorker? active;
  [SameObject] readonly attribute NavigationPreloadManager navigationPreload;
  readonly attribute USVString scope;
  readonly attribute ServiceWorkerUpdateViaCache updateViaCache;
  Promise<ServiceWorkerRegistration> update();
  Promise<boolean> unregister();
  attribute EventHandler onupdatefound;
};
enum ServiceWorkerUpdateViaCache { "imports", "all", "none" };

[SecureContext, Exposed=Window]
interface ServiceWorkerContainer : EventTarget {
  readonly attribute ServiceWorker? controller;
  readonly attribute Promise<ServiceWorkerRegistration> ready;
  Promise<ServiceWorkerRegistration> register(USVString scriptURL, optional RegistrationOptions options = {});
  Promise<(ServiceWorkerRegistration or undefined)> getRegistration(optional USVString clientURL = "");
  Promise<FrozenArray<ServiceWorkerRegistration>> getRegistrations();
  undefined startMessages();
  attribute EventHandler oncontrollerchange;
  attribute EventHandler onmessage;
  attribute EventHandler onmessageerror;
};
dictionary RegistrationOptions {
  USVString scope;
  WorkerType type = "classic";
  ServiceWorkerUpdateViaCache updateViaCache = "imports";
};
enum WorkerType { "classic", "module" };

[SecureContext, Exposed=(Window,Worker)]
interface NavigationPreloadManager {
  Promise<undefined> enable();
  Promise<undefined> disable();
  Promise<undefined> setHeaderValue(ByteString value);
  Promise<NavigationPreloadState> getState();
};
dictionary NavigationPreloadState { boolean enabled = false; ByteString headerValue; };

[Global=(Worker,ServiceWorker), Exposed=ServiceWorker, SecureContext]
interface ServiceWorkerGlobalScope : WorkerGlobalScope {
  [SameObject] readonly attribute Clients clients;
  [SameObject] readonly attribute ServiceWorkerRegistration registration;
  [SameObject] readonly attribute ServiceWorker serviceWorker;
  Promise<undefined> skipWaiting();
  attribute EventHandler oninstall;
  attribute EventHandler onactivate;
  attribute EventHandler onfetch;
  attribute EventHandler onmessage;
  attribute EventHandler onmessageerror;
};

[Exposed=ServiceWorker, SecureContext]
interface Client {
  readonly attribute USVString url;
  readonly attribute FrameType frameType;
  readonly attribute DOMString id;
  readonly attribute ClientType type;
  undefined postMessage(any message, sequence<object> transfer);
  undefined postMessage(any message, optional StructuredSerializeOptions options = {});
};
[Exposed=ServiceWorker, SecureContext]
interface WindowClient : Client {
  readonly attribute DocumentVisibilityState visibilityState;
  readonly attribute boolean focused;
  [SameObject] readonly attribute FrozenArray<USVString> ancestorOrigins;
  Promise<WindowClient> focus();
  Promise<WindowClient?> navigate(USVString url);
};
enum FrameType { "auxiliary", "top-level", "nested", "none" };
enum ClientType { "window", "worker", "sharedworker", "all" };

[Exposed=ServiceWorker, SecureContext]
interface Clients {
  Promise<(Client or undefined)> get(DOMString id);
  Promise<FrozenArray<Client>> matchAll(optional ClientQueryOptions options = {});
  Promise<WindowClient?> openWindow(USVString url);
  Promise<undefined> claim();
};
dictionary ClientQueryOptions { boolean includeUncontrolled = false; ClientType type = "window"; };

[Exposed=ServiceWorker, SecureContext]
interface ExtendableEvent : Event {
  constructor(DOMString type, optional ExtendableEventInit eventInitDict = {});
  undefined waitUntil(Promise<any> f);
};
dictionary ExtendableEventInit : EventInit {};

[Exposed=ServiceWorker, SecureContext]
interface FetchEvent : ExtendableEvent {
  constructor(DOMString type, FetchEventInit eventInitDict);
  [SameObject] readonly attribute Request request;
  readonly attribute Promise<any> preloadResponse;
  readonly attribute DOMString clientId;
  readonly attribute DOMString resultingClientId;
  readonly attribute DOMString replacesClientId;
  readonly attribute Promise<undefined> handled;
  undefined respondWith(Promise<Response> r);
};
dictionary FetchEventInit : ExtendableEventInit {
  required Request request;
  Promise<any> preloadResponse;
  DOMString clientId = "";
  DOMString resultingClientId = "";
  DOMString replacesClientId = "";
  Promise<undefined> handled;
};

[Exposed=ServiceWorker, SecureContext]
interface ExtendableMessageEvent : ExtendableEvent {
  constructor(DOMString type, optional ExtendableMessageEventInit eventInitDict = {});
  readonly attribute any data;
  readonly attribute USVString origin;
  readonly attribute DOMString lastEventId;
  readonly attribute (Client or ServiceWorker or MessagePort)? source;
  readonly attribute FrozenArray<MessagePort> ports;
};

[SecureContext, Exposed=(Window,Worker)]
interface CacheStorage {
  Promise<any> match(RequestInfo request, optional MultiCacheQueryOptions options = {});
  Promise<boolean> has(DOMString cacheName);
  Promise<Cache> open(DOMString cacheName);
  Promise<boolean> delete(DOMString cacheName);
  Promise<sequence<DOMString>> keys();
};
[SecureContext, Exposed=(Window,Worker)]
interface Cache {
  Promise<any> match(RequestInfo request, optional CacheQueryOptions options = {});
  Promise<FrozenArray<Response>> matchAll(optional RequestInfo request, optional CacheQueryOptions options = {});
  Promise<undefined> add(RequestInfo request);
  Promise<undefined> addAll(sequence<RequestInfo> requests);
  Promise<undefined> put(RequestInfo request, Response response);
  Promise<boolean> delete(RequestInfo request, optional CacheQueryOptions options = {});
  Promise<FrozenArray<Request>> keys(optional RequestInfo request, optional CacheQueryOptions options = {});
};
dictionary CacheQueryOptions { boolean ignoreSearch = false; boolean ignoreMethod = false; boolean ignoreVary = false; };
dictionary MultiCacheQueryOptions : CacheQueryOptions { DOMString cacheName; };

partial interface mixin WindowOrWorkerGlobalScope {
  [SecureContext, SameObject] readonly attribute CacheStorage caches;
};
partial interface Navigator {
  [SecureContext, SameObject] readonly attribute ServiceWorkerContainer serviceWorker;
};
```

The Fetch subset (`Headers`, `Request`, `Response`, `Body`) and the DOM shim (`EventTarget`, `Event`, `DOMException`, `MessageEvent`, `ErrorEvent`, `AbortController`, `AbortSignal`) follow their own specifications; §5.4 and §5.5 define which members are in scope.

### Appendix B. Expected source tree

```
crates/boa_sw_core/src/
├── lib.rs
├── error.rs           ids.rs        key.rs        clock.rs      observe.rs
├── url_util.rs        model.rs      registry.rs   invariants.rs
├── storage.rs         storage/contract.rs
├── event.rs           job.rs        core.rs
├── algo/{mod,register,update,install,activate,unregister,state,clients,recovery}.rs
├── fetch/{mod,model,handle,validate}.rs
└── cache/{mod,key,query,batch,storage}.rs

crates/boa_sw_dom/src/
└── {lib,event,event_target,exception,message_event,error_event,abort,register,helpers}.rs

crates/boa_sw_fetch/src/
└── {lib,headers,request,response,body,fetch,convert}.rs

crates/boa_sw/src/
├── lib.rs             runtime.rs    install.rs    realm_util.rs  pump.rs
├── commands.rs        tickets.rs    notify.rs     test_util.rs
├── api/{mod,container,registration,worker,navigation_preload,clients,client,cache,cache_storage}.rs
├── worker/{mod,global,start,imports,module,terminate}.rs
├── events/{mod,extendable,dispatch,elp,fetch_event,message_event}.rs
├── fetch/{mod,interception,preload}.rs
└── messaging/{mod,serialize,queues,port,channel,port_table}.rs

crates/boa_sw_memory/src/lib.rs
crates/boa_sw_sqlite/src/{lib,schema,migrate,statements,blobs,lock}.rs
crates/boa_sw_wpt/src/{lib,main,harness,server,expectations}.rs
```

### Appendix C. SQLite schema (v1)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE schema_version (version INTEGER NOT NULL);

CREATE TABLE registrations (
  storage_key            TEXT    NOT NULL,
  scope                  TEXT    NOT NULL,
  registration_id        INTEGER NOT NULL,
  update_via_cache       INTEGER NOT NULL,          -- 0 imports, 1 all, 2 none
  installing_worker      INTEGER NULL REFERENCES workers(worker_id) ON DELETE SET NULL,
  waiting_worker         INTEGER NULL REFERENCES workers(worker_id) ON DELETE SET NULL,
  active_worker          INTEGER NULL REFERENCES workers(worker_id) ON DELETE SET NULL,
  last_update_check_ms   INTEGER NULL,
  nav_preload_enabled    INTEGER NOT NULL DEFAULT 0,
  nav_preload_header     TEXT    NOT NULL DEFAULT 'true',
  uninstalling           INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (storage_key, scope)
) WITHOUT ROWID;
CREATE UNIQUE INDEX idx_reg_id ON registrations(registration_id);

CREATE TABLE workers (
  worker_id        INTEGER PRIMARY KEY,
  storage_key      TEXT    NOT NULL,
  registration_id  INTEGER NOT NULL,
  script_url       TEXT    NOT NULL,
  worker_type      INTEGER NOT NULL,                -- 0 classic, 1 module
  state            INTEGER NOT NULL,
  has_fetch_handler INTEGER NULL,                   -- NULL unknown, 0 no, 1 yes
  handled_events   TEXT    NOT NULL DEFAULT '',     -- comma-separated
  created_ms       INTEGER NOT NULL
);
CREATE INDEX idx_workers_reg ON workers(storage_key, registration_id);

CREATE TABLE scripts (
  worker_id   INTEGER NOT NULL REFERENCES workers(worker_id) ON DELETE CASCADE,
  url         TEXT    NOT NULL,
  is_main     INTEGER NOT NULL,
  mime        TEXT    NOT NULL,
  sw_allowed  TEXT    NULL,
  sha256      BLOB    NOT NULL,
  body        BLOB    NOT NULL,
  PRIMARY KEY (worker_id, url)
) WITHOUT ROWID;

CREATE TABLE caches (
  cache_id    INTEGER PRIMARY KEY,
  storage_key TEXT    NOT NULL,
  name        TEXT    NOT NULL,
  created_seq INTEGER NOT NULL,
  UNIQUE (storage_key, name)
);

CREATE TABLE cache_entries (
  entry_id      INTEGER PRIMARY KEY,
  cache_id      INTEGER NOT NULL REFERENCES caches(cache_id) ON DELETE CASCADE,
  insert_seq    INTEGER NOT NULL,
  request_url   TEXT    NOT NULL,                   -- fragment excluded
  request_url_nosearch TEXT NOT NULL,               -- for ignoreSearch queries
  request_method TEXT   NOT NULL,
  request_headers BLOB  NOT NULL,                   -- serialized header list
  vary_names    TEXT    NOT NULL DEFAULT '',        -- lowercased, comma-separated; '*' possible
  response_kind INTEGER NOT NULL,
  response_status INTEGER NOT NULL,
  response_status_text TEXT NOT NULL,
  response_url_list TEXT NOT NULL,
  response_headers BLOB NOT NULL,
  body_inline   BLOB    NULL,                       -- when <= 64 KiB
  body_ref      INTEGER NULL REFERENCES cache_bodies(body_id) ON DELETE CASCADE,
  byte_size     INTEGER NOT NULL
);
CREATE INDEX idx_entries_lookup   ON cache_entries(cache_id, request_url, request_method);
CREATE INDEX idx_entries_nosearch ON cache_entries(cache_id, request_url_nosearch, request_method);
CREATE INDEX idx_entries_order    ON cache_entries(cache_id, insert_seq);

CREATE TABLE cache_bodies (body_id INTEGER PRIMARY KEY, bytes BLOB NOT NULL);

CREATE TABLE usage_counter (storage_key TEXT PRIMARY KEY, bytes INTEGER NOT NULL);
```

`R18.C.1` — every query MUST use one of the three indexes above; the query-plan test of T-22 enforces it.

### Appendix D. Acceptance scenario (must run end-to-end)

`sw.js`:
```js
const CACHE = 'v1';
self.addEventListener('install', (e) => {
  e.waitUntil(caches.open(CACHE).then((c) => c.addAll(['/', '/app.js', '/style.css'])));
});
self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    for (const name of await caches.keys()) if (name !== CACHE) await caches.delete(name);
    await self.clients.claim();
  })());
});
self.addEventListener('fetch', (e) => {
  e.respondWith((async () => {
    const hit = await caches.match(e.request);
    if (hit) return hit;
    try {
      const res = await fetch(e.request);
      if (res.ok && e.request.method === 'GET') {
        const c = await caches.open(CACHE);
        await c.put(e.request, res.clone());
      }
      return res;
    } catch {
      return (await caches.match('/')) || Response.error();
    }
  })());
});
self.addEventListener('message', (e) => {
  if (e.data === 'ping') e.source.postMessage('pong');
  if (e.data && e.data.q === 'ping' && e.ports[0]) e.ports[0].postMessage('pong');
  if (e.data === 'skip') e.waitUntil(self.skipWaiting());
});
```

Page script steps, each asserted by the demo host:
1. `const reg = await navigator.serviceWorker.register('/sw.js');` → resolves; `reg.installing` is non-null.
2. `await navigator.serviceWorker.ready` → resolves; `reg.active.state === 'activated'`.
3. Host performs a navigation fetch for `/` → intercepted, served from cache, client becomes controlled; `navigator.serviceWorker.controller !== null`.
4. Host performs a subresource fetch for `/app.js` → served from cache; `FakeHttp` shows no new network request.
5. Host performs a fetch for `/api/data` (not cached) → passes through to the network and is cached.
6. Network is switched off; the same fetches still succeed from cache; `/unknown` falls back to `/`.
7. `navigator.serviceWorker.controller.postMessage('ping')` → the page receives `'pong'`.
7a. The page creates a `MessageChannel`, sends `controller.postMessage({q:'ping'}, [ch.port2])`, and the worker replies on `event.ports[0]` → the page receives the reply on `ch.port1.onmessage` (covers `D-3`).
8. The script bytes are changed; `await reg.update()` → a new worker installs and stays `waiting`.
9. `reg.waiting.postMessage('skip')` → activation happens, `controllerchange` fires, `reg.active` is the new worker.
10. Process restart (`shutdown` → `install`): the registration and caches survive; step 3 still serves from cache without re-fetching the script.
11. `await reg.unregister()` → `true`; a new navigation is no longer intercepted; after the client unloads, all rows for the registration are gone from storage.

### Appendix E. Traceability checklist (Spec algorithm → module)

| Spec algorithm | Module | Task |
|---|---|---|
| Start Register / Register | `algo/register.rs` | T-06 |
| Schedule Job / Run Job / Finish Job / job equivalence | `job.rs` | T-05 |
| Update / Soft Update | `algo/update.rs` | T-06 |
| Install | `algo/install.rs` | T-07 |
| Activate / Try Activate | `algo/activate.rs` | T-07 |
| Unregister / Clear Registration / Try Clear Registration | `algo/unregister.rs`, `algo/activate.rs` | T-07 |
| Update Registration State / Update Worker State / Notify Controller Change | `algo/state.rs` + `notify.rs` | T-07, T-16 |
| Scope Match / Match Service Worker Registration / Get Newest Worker | `url_util.rs`, `registry.rs` | T-03, T-04 |
| Handle Fetch | `fetch/handle.rs` + `fetch/interception.rs` | T-09, T-18 |
| Handle Functional Event / Should Skip Event | `events/dispatch.rs`, `fetch/handle.rs` | T-15, T-09 |
| Run Service Worker / Terminate Service Worker / Has No Pending Events | `worker/start.rs`, `worker/terminate.rs`, `algo/state.rs` | T-14, T-07 |
| Update Extended Lifetime Promises | `events/elp.rs` | T-15 |
| Handle Service Worker Client Unload | `algo/clients.rs` | T-07 |
| Handle User Agent Shutdown | `algo/recovery.rs` | T-07, T-23 |
| Query Cache / Request Matches Cached Item / Batch Cache Operations | `cache/query.rs`, `cache/batch.rs` | T-10 |
| skipWaiting / claim | `algo/clients.rs`, `api/clients.rs` | T-07, T-17 |
| Navigation preload | `fetch/preload.rs`, `api/navigation_preload.rs` | T-18, T-16 |
| Structured serialize/deserialize for messages | `messaging/serialize.rs` | T-20 |
| `MessageChannel`/`MessagePort` entanglement, transfer, delivery | `messaging/{port,channel,port_table}.rs` | T-27 |

`docs/traceability.md` extends this table with one column: the test names covering each row.

### Appendix F. Work-order template (`tasks/<NN>_TASK_<NAME>.md`)

```markdown
# WORK ORDER: T-<NN> — <title>

| Metadata | Value |
|---|---|
| Task id | T-<NN> |
| Milestone | M<n> |
| Target crates | … |
| Normative TS | TZ_boa_sw_ServiceWorkers.md §… |
| Prerequisites | accepted T-<NN> … |
| Diff budget | ~N lines |

## 1. Guardrails
Copy the relevant `AD-*` decisions and the borrow-discipline rules (§3.4) verbatim.

## 2. Files to create/modify
Exact paths, one line each, with a one-line purpose.

## 3. Signatures
Every public type and function, copied from the TS. The implementer MUST NOT change them.

## 4. Algorithms
Numbered steps copied from the TS, with the requirement ids they satisfy.

## 5. Tests to write
Exact test names and what each asserts.

## 6. Acceptance criteria
Numbered, each verifiable by a command; include the commands.

## 7. Do not do
Explicit out-of-scope list.

## 8. Handoff
Write `docs/reviews/T-<NN>-handoff.md`: what was built, how to run the demo commands,
deviations (should be none), `docs/DECISIONS.md` entries added, open questions.
```
