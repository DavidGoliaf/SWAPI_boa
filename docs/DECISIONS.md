# Decision log (ADR)

Format: one section per decision. Architectural decisions `AD-*` and customer decisions `D-*` are
copied from `TZ_boa_sw_ServiceWorkers.md` (§3.2, §17.3) and are **not** open for revision by an
implementer — see `AGENTS.md`. Decisions taken *during* implementation (including every new
dependency, per `R2.3.1`) are appended below them with the id `I-<NN>`.

## AD-1 — Realm per service worker, one Context, one thread

Each *running* service worker instance owns a dedicated Boa realm created with `Context::create_realm()`. The realm's global object is turned into a `ServiceWorkerGlobalScope` (§5.2.1). Client JavaScript keeps running in the host's realm. All realms live in a single `Context` on a single OS thread; there is no parallelism between a worker and its clients. Rationale: `Context` is `!Send`; realms give per-worker globals and per-realm class registration at a fraction of the complexity of multi-context/multi-thread; `NativeAsyncJob::with_realm` already carries realm affinity. Every cross-realm interaction MUST nevertheless go through the message/ticket boundary defined in `AD-6`, so that a future thread-per-worker mode is a runtime change only, not an API change.

## AD-2 — Engine-free core

All model state and all Spec algorithms that do not require executing JavaScript live in `boa_sw_core` and are expressed over plain Rust types. `boa_sw_core` never sees `JsValue`. Anything the algorithms need from JS (e.g. "did the install handler's promise reject?") is passed in as a plain Rust outcome value.

## AD-3 — Everything environmental is injected

Network, clients, clock, storage, and random ids are traits supplied by the host (§4.2). Core code MUST NOT read the system clock, open sockets or touch the filesystem. Tests use deterministic fakes for all four.

## AD-4 — Deterministic, non-async core; async only at the edges

Core algorithms are synchronous state machines: they consume *events* (`CoreEvent`) and produce *commands* (`CoreCommand`). Waiting on the network, on script evaluation or on a promise is expressed as "issue command, park the job, resume on the matching event". No `async fn`, no futures, no callbacks inside `boa_sw_core`. Rationale: a state machine is exhaustively testable without an engine and is trivially replayable.

## AD-5 — Job queue per scope, strictly FIFO

Registration jobs (`register`, `update`, `unregister`) are queued per `(storage key, scope url)` and executed one at a time in insertion order, exactly as `#job-queue` requires, including job equivalence and promise coalescing.

## AD-6 — Structured clone at every realm boundary

`postMessage` payloads, `ExtendableMessageEvent.data`, and any value crossing from a client realm to a SW realm (or back) MUST be serialized with `boa_wintertc::store::JsValueStore` (feature `runtime-interop`) or the crate's own equivalent when that feature is off. Passing a `JsObject` created in one realm into another realm is a defect.

## AD-7 — Buffered bodies only

Request and response bodies are `Rc<Vec<u8>>`. There are no streams. A configurable `max_body_bytes` (default 32 MiB) causes `HttpClient` results larger than the cap to be turned into a network error, and `Response`/`Request` constructors to throw `RangeError`.

## AD-8 — `boa-sw` owns the Fetch classes

`Headers`, `Request`, `Response` and `fetch` used by Service Workers come from `boa_sw_fetch`, because the Spec needs request `mode`/`credentials`/`destination`/`cache` and response `type`/`redirected`/`url`, which `boa_runtime::fetch` does not model. On registration, `boa_sw` MUST detect an existing global `Request`/`Response`/`Headers`/`fetch` in the target realm and fail with a clear error (`SwError::ConflictingGlobals`) unless the host explicitly opted in via `SwConfig::allow_global_overwrite` or disabled `fetch-classes`.

## AD-9 — All time comes from `Clock`

Last-update-check timestamps, the 86 400 s soft-update rule, idle timeouts, and event timeouts read `Clock::now_ms()`/`monotonic_ms()`. Tests drive a `FakeClock`.

## AD-10 — Cooperative termination

Terminating a worker means: reject its outstanding ELPs, drop its realm and pending jobs, mark the worker record as not running. Interrupting JavaScript that is currently on the stack is out of scope; the runtime exposes a budget hook the host may wire to Boa's interrupt facilities when available (§11.5).

## AD-11 — One storage trait family, atomic batches

Registrations, script resource maps and caches are persisted through a single `SwStorage` trait (§4.2.4). A lifecycle transition that must be durable (install completed, activation completed, registration cleared) is written as one `StorageBatch` applied atomically. A crash between batches MUST leave a state that recovery (§7.11) can repair.

## AD-12 — Explicit error taxonomy

`boa_sw_core::SwError` is the single internal error type; §12 gives the total mapping from `SwError` variants to JavaScript exceptions (`DOMException` name or native error). Bindings MUST NOT invent messages or names outside that table.

## D-1 — SQLite is the default storage backend

Default storage backend for docs, examples and CI is **SQLite.** `sqlite-backend` is in `default-features`; `boa_sw_memory` remains for tests and for hosts that opt out (`--no-default-features`). A host that selects the memory backend is warned once at `install` (TS §2.4, `R2.4.2`, T-22, T-23, §17.1).

## D-2 — `caches` is exposed in the client realm by default

**Yes, by default**, per the Spec's `WindowOrWorkerGlobalScope` mixin. `SwConfig::caches_in_client_realm = true`; turning it off is the host's documented deviation (TS `R5.3.1`, T-19).

## D-3 — `MessageChannel`/`MessagePort` in v1.0

**Yes, full support**, including port transfer in both directions and a populated `ExtendableMessageEvent.ports`. Rationale: the `postMessage(msg, [port2])` + `port1.onmessage` reply pattern used by real service-worker libraries fails *silently* without it, which breaks `R1.1.1` (TS §1.3 item 2a, `R5.5.7`, §10.4, task T-27).

## D-4 — WPT contract

**Two-part:** ≥ 85 % of runnable subtests at release **and** every file in `crates/boa_sw_wpt/priority.txt` 100 % green with no expectation entries. A bare percentage is gameable by filtering; the file list is not. Streams/`Blob`/iframe-client work (≈ +6 person-weeks for ≥ 92 %) is **not** in v1.0 (TS `R15.3.3`, §17.1 item 2, T-24).

## D-5 — The host owns ClientId lifecycle

**The host owns it, per the Spec:** a `ClientId` identifies a *document*, not a tab. The host allocates a fresh id per navigation and supplies `reserved_client_id`/`replaces_client_id`; the runtime never generates ids. Enforced at runtime by the host-contract validator (TS `R6.1.2a`, `R8.4.1`, T-23).

## I-01 — <first implementation decision>

(none yet)
