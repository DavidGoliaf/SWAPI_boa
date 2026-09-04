# WORK ORDER: T-02 — Error taxonomy, ids, storage key, clock, observer

| Metadata | Value |
|---|---|
| **Task id** | `T-02-ERROR-IDS-KEY-CLOCK` |
| **Milestone** | M0 |
| **Target crates** | `crates/boa_sw_core` only |
| **Normative TS** | `TZ_boa_sw_ServiceWorkers.md` §12 (error model), §6.1 (ids, storage key), §4.2.1 (`Clock`), §4.2.5 + `R4.2.11` (observer), §13.1 (`R13.1`), §2.1–2.4 |
| **Prerequisites** | `T-01` **accepted** (workspace, manifests, lints, CI exist) |
| **Diff budget** | ~600 lines (stop and escalate above ~900) |
| **Architect's role** | Every public item is specified below with its exact signature and behaviour. |
| **Implementer's role** | Implement exactly these items and their tests. **No other types.** No registry, no jobs, no algorithms — those are `T-04`+. |

---

## 1. Guardrails

1. **`AD-2` — engine-free core.** Nothing in this task may reference `boa_engine`, `boa_gc`,
   `boa_runtime` or `boa_wintertc`. `T-01`'s CI job `dependency-direction` enforces it; do not
   weaken it.
2. **`AD-3` — everything environmental is injected.** `Clock` is a trait. The only place allowed to
   read the machine clock is `SystemClock`, behind feature `std`. No `SystemTime::now()` anywhere
   else, ever.
3. **`AD-12` — the error taxonomy is the single source of truth.** Bindings will map errors to
   JavaScript *from `js_kind()` alone*. If a mapping feels wrong, escalate — do not "fix" it locally.
4. **`R2.1.2`** — no `unwrap`/`expect`/`panic!`/`todo!` in library code. Documented-infallible cases
   carry a comment naming the invariant.
5. **`R2.1.3`** — no blocking I/O, no threads, no allocation-heavy helpers on hot paths.
6. **Exhaustiveness is a feature.** `SwError` is deliberately **not** `#[non_exhaustive]`: adding a
   variant must break `js_kind()` and the mapping test. `ObserverEvent` **is** `#[non_exhaustive]`
   (`R4.2.11`), because later tasks extend it.
7. Integration-test files (`crates/*/tests/*.rs`) start with
   `#![allow(clippy::unwrap_used, clippy::expect_used)]` (convention set in `T-01`).

---

## 2. Files to create / modify

```
crates/boa_sw_core/src/lib.rs        # MODIFY: replace the stub body with module declarations + re-exports
crates/boa_sw_core/src/error.rs      # NEW
crates/boa_sw_core/src/ids.rs        # NEW
crates/boa_sw_core/src/key.rs        # NEW
crates/boa_sw_core/src/clock.rs      # NEW
crates/boa_sw_core/src/observe.rs    # NEW
crates/boa_sw_core/tests/public_api.rs   # NEW: compile-level check of the re-exported surface
docs/traceability.md                 # MODIFY: append this task's rows
docs/reviews/T-02-handoff.md         # NEW
```

Nothing else. Do not touch other crates, manifests, `deny.toml`, or CI.

---

## 3. Signatures and behaviour (normative — copy, do not redesign)

### 3.1. `src/lib.rs`

Keep the existing doc comment and lint attributes; replace the "Status: skeleton" line with
`//! **Status:** foundations (`T-02`): errors, ids, storage key, clock, observer.` and append:

```rust
pub mod clock;
pub mod error;
pub mod ids;
pub mod key;
pub mod observe;

pub use clock::Clock;
pub use error::{
    JsErrorKind, NetworkError, ScriptEvalError, ScriptFetchError, StorageError, StorageResult,
    SwError, SwResult,
};
pub use ids::{CacheId, ClientId, DispatchId, IdAllocator, JobId, RegistrationId, WorkerId};
pub use key::{StorageKey, is_potentially_trustworthy};
pub use observe::{NullObserver, ObserverEvent, SwObserver, WarningCode};
```

### 3.2. `src/error.rs`

```rust
/// Result alias used across the crate.
pub type SwResult<T> = Result<T, SwError>;

/// The single internal error type of `boa-sw` (TS §12, `AD-12`).
///
/// Deliberately **not** `#[non_exhaustive]`: every consumer matches exhaustively, so adding a
/// variant is a compile error until the mapping in [`SwError::js_kind`] is extended too.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SwError {
    #[error("invalid URL: {0}")]                     InvalidUrl(String),
    #[error("cross-origin operation is not allowed")] CrossOrigin,
    #[error("insecure context")]                     InsecureContext,
    #[error("path restriction violated")]            PathRestriction,
    #[error("unsupported script MIME type: {0}")]    BadScriptMime(String),
    #[error("script fetch failed: {0}")]             ScriptFetch(String),
    #[error("script redirected")]                    ScriptRedirect,
    #[error("script evaluation failed: {0}")]        ScriptEval(String),
    #[error("install failed: {0}")]                  InstallFailed(String),
    #[error("registration is invalid or removed")]   InvalidState,
    #[error("operation aborted")]                    Aborted,
    #[error("timed out")]                            TimedOut,
    #[error("network error: {0}")]                   Network(String),
    #[error("quota exceeded")]                       QuotaExceeded,
    #[error("storage failure: {0}")]                 Storage(String),
    #[error("value cannot be cloned")]               DataClone,
    #[error("not supported: {0}")]                   NotSupported(String),
    #[error("globals already defined: {name}")]      ConflictingGlobals { name: String },
    #[error("runtime already installed")]            AlreadyInstalled,
    #[error("re-entrant pump")]                      Reentrant,
    #[error("internal invariant violated: {0}")]     Internal(String),
}

/// How an [`SwError`] surfaces to JavaScript (TS §12).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum JsErrorKind {
    /// A native `TypeError`.
    TypeError,
    /// A native `RangeError`.
    RangeError,
    /// A `DOMException` with this name.
    Dom(&'static str),
    /// Never reaches JavaScript; returned to the host as a `Result`.
    HostOnly,
}

impl SwError {
    /// Total mapping to the JavaScript surface. Exhaustive by construction — no wildcard arm.
    #[must_use]
    pub fn js_kind(&self) -> JsErrorKind;

    /// `Some(name)` when [`Self::js_kind`] is [`JsErrorKind::Dom`], `None` otherwise.
    #[must_use]
    pub fn dom_name(&self) -> Option<&'static str>;

    /// The message used when the error carries none of its own (TS §12 table).
    #[must_use]
    pub fn default_message(&self) -> String;
}
```

**The mapping (`js_kind`) — normative, copy exactly:**

| Variant | `JsErrorKind` | `default_message()` |
|---|---|---|
| `InvalidUrl(_)` | `TypeError` | `"Failed to parse URL"` |
| `CrossOrigin` | `Dom("SecurityError")` | `"The origin of the provided scriptURL does not match the current origin"` |
| `InsecureContext` | `Dom("SecurityError")` | `"Service workers are only available in secure contexts"` |
| `PathRestriction` | `Dom("SecurityError")` | `"The path of the provided scope is not under the max scope allowed"` |
| `BadScriptMime(_)` | `Dom("SecurityError")` | `"The script has an unsupported MIME type"` |
| `ScriptFetch(_)` | `TypeError` | `"Failed to fetch a service worker script"` |
| `ScriptRedirect` | `Dom("SecurityError")` | `"The script resource is behind a redirect, which is disallowed"` |
| `ScriptEval(_)` | `TypeError` | `"Failed to evaluate the service worker script"` |
| `InstallFailed(_)` | `TypeError` | `"The service worker installation failed"` |
| `InvalidState` | `Dom("InvalidStateError")` | `"The registration is in an invalid state"` |
| `Aborted` | `Dom("AbortError")` | `"The operation was aborted"` |
| `TimedOut` | `Dom("AbortError")` | `"The operation timed out"` |
| `Network(_)` | `TypeError` | `"A network error occurred"` |
| `QuotaExceeded` | `Dom("QuotaExceededError")` | `"The storage quota has been exceeded"` |
| `Storage(_)` | `Dom("UnknownError")` | `"A storage error occurred"` |
| `DataClone` | `Dom("DataCloneError")` | `"The value could not be cloned"` |
| `NotSupported(_)` | `Dom("NotSupportedError")` | `"The operation is not supported"` |
| `ConflictingGlobals { .. }` | `HostOnly` | `"A conflicting global is already defined"` |
| `AlreadyInstalled` | `HostOnly` | `"The runtime is already installed in this context"` |
| `Reentrant` | `HostOnly` | `"pump() was called re-entrantly"` |
| `Internal(_)` | `Dom("UnknownError")` | `"An internal error occurred"` |

> `ScriptRedirect → SecurityError` matches `R6.4.2`. The pinned Spec revision
> (`docs/SPEC_REVISION.md`) is the tiebreaker: if its Update algorithm turns a redirected script
> into a plain network error instead, **do not change the table** — raise `Q-<NN>` and continue.

**Companion error types** (used by later tasks; define them now so signatures stop shifting):

```rust
/// Storage-layer failures (TS §4.2.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("not found")]                          NotFound,
    #[error("conflict: {0}")]                      Conflict(String),
    #[error("quota exceeded")]                     QuotaExceeded,
    #[error("data is corrupt: {0}")]               Corrupt(String),
    #[error("storage is locked by another process")] Locked,
    #[error("I/O failure: {0}")]                   Io(String),
    #[error("unsupported by this backend: {0}")]   Unsupported(String),
}

/// Result alias for storage operations (TS §4.2.4).
pub type StorageResult<T> = Result<T, StorageError>;

/// Failures of a network fetch, as seen by the runtime and the host (TS §4.2.2, §8.2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    #[error("network request failed: {0}")]        Failed(String),
    #[error("request timed out")]                  Timeout,
    #[error("request aborted")]                    Aborted,
    #[error("body of {actual} bytes exceeds the limit of {limit}")]
                                                   BodyTooLarge { actual: u64, limit: u64 },
    #[error("the fetch handler threw")]            HandlerThrew,
    #[error("the respondWith promise rejected")]   HandlerRejected,
    #[error("respondWith produced an invalid response: {0}")] InvalidResponse(String),
}

/// Failures while fetching a script resource (TS §6.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptFetchError {
    #[error(transparent)]                          Network(#[from] NetworkError),
    #[error("unexpected HTTP status {0}")]         BadStatus(u16),
    #[error("unsupported MIME type: {0}")]         BadMime(String),
    #[error("the script response is a redirect")]  Redirected,
    #[error("script of {actual} bytes exceeds the limit of {limit}")]
                                                   TooLarge { actual: u64, limit: u64 },
    #[error("{actual} imported scripts exceed the limit of {limit}")]
                                                   TooManyImports { actual: u32, limit: u32 },
}

/// Failures while evaluating a script (TS §11.4.5).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptEvalError {
    #[error("parse error: {message}")]             Parse { message: String },
    #[error("uncaught exception: {message}")]      Throw { message: String },
    #[error("the worker was terminated during evaluation")] Terminated,
}
```

**`From` conversions into `SwError`** (implement exactly these, nothing more):

| From | To |
|---|---|
| `StorageError::QuotaExceeded` | `SwError::QuotaExceeded` |
| `StorageError::*` (all others) | `SwError::Storage(<the Display text>)` |
| `NetworkError::Timeout` | `SwError::TimedOut` |
| `NetworkError::Aborted` | `SwError::Aborted` |
| `NetworkError::*` (all others) | `SwError::Network(<Display>)` |
| `ScriptFetchError::BadMime(m)` | `SwError::BadScriptMime(m)` |
| `ScriptFetchError::Redirected` | `SwError::ScriptRedirect` |
| `ScriptFetchError::TooLarge{..}` / `TooManyImports{..}` | `SwError::QuotaExceeded` |
| `ScriptFetchError::*` (all others) | `SwError::ScriptFetch(<Display>)` |
| `ScriptEvalError::*` | `SwError::ScriptEval(<Display>)` |

### 3.3. `src/ids.rs`

```rust
/// Declares an opaque, `u64`-backed identifier newtype.
///
/// Generated items: the struct, `from_raw`, `raw`, `Display` (as `<prefix>#<n>`),
/// `Copy`/`Clone`/`PartialEq`/`Eq`/`Hash`/`PartialOrd`/`Ord`/`Debug`.
macro_rules! define_id { /* … */ }

define_id!(RegistrationId, "reg");
define_id!(WorkerId, "sw");
define_id!(JobId, "job");
define_id!(DispatchId, "dispatch");
define_id!(CacheId, "cache");

/// A host-assigned client identifier (TS `R6.1.2`, `R6.1.2a`).
///
/// Opaque: this crate never parses it, never generates one, and never reuses one.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ClientId(String);

impl ClientId {
    pub fn new(raw: impl Into<String>) -> Self;
    #[must_use] pub fn as_str(&self) -> &str;
}
impl core::fmt::Display for ClientId { /* the raw string, unquoted */ }

/// Monotonic id source (TS `R6.1.1`).
#[derive(Debug, Clone)]
pub struct IdAllocator { next: u64 }

impl IdAllocator {
    /// Starts allocating at 1; `0` is never handed out and can be used as a "none" marker.
    #[must_use] pub const fn new() -> Self;
    /// Returns the next raw id.
    pub fn allocate(&mut self) -> u64;
    /// Continues above a persisted maximum: `next = max(next, max_seen + 1)` (TS `R6.1.1`).
    pub fn restore_from(&mut self, max_seen: u64);
    /// The id `allocate` would return next, without consuming it.
    #[must_use] pub const fn peek(&self) -> u64;
}

impl Default for IdAllocator { /* MUST return `Self::new()`, i.e. next = 1, not 0 */ }
```

`allocate` uses `checked_add(1)`; exhausting `u64` is unreachable in practice (2^64 allocations),
so on overflow it saturates and a `debug_assert!` fires — write the invariant as a comment
(`R2.1.2` allows this; no `panic!` in release).

### 3.4. `src/key.rs`

```rust
/// Storage isolation key (TS §6.1). Normally the serialized origin of the client.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct StorageKey(String);

impl StorageKey {
    /// Serializes `url`'s origin. Opaque origins (`data:`, `blob:` without an inner origin,
    /// `about:`) are rejected with [`SwError::InvalidUrl`].
    pub fn from_origin(url: &Url) -> SwResult<Self>;
    /// Wraps a host-supplied key verbatim; no validation, no normalization.
    pub fn from_raw(raw: impl Into<String>) -> Self;
    #[must_use] pub fn as_str(&self) -> &str;
}
impl core::fmt::Display for StorageKey { /* the raw string */ }

/// Whether `url`'s origin is potentially trustworthy (TS `R13.1`, W3C Secure Contexts).
///
/// Steps:
/// 1. scheme is `https` or `wss` → true
/// 2. host is an IPv4 address in `127.0.0.0/8`, or the IPv6 address `::1` → true
/// 3. host is `localhost` or ends with `.localhost` (ASCII case-insensitive) → true
/// 4. scheme is `file` → true
/// 5. otherwise → false
#[must_use]
pub fn is_potentially_trustworthy(url: &Url) -> bool;
```

Use `url::Url::host()` and match `url::Host::{Domain, Ipv4, Ipv6}`; do not parse the host string by
hand. `from_origin` uses `Url::origin()` and `Origin::ascii_serialization()`, rejecting
`Origin::Opaque`.

### 3.5. `src/clock.rs`

```rust
/// Time source (TS §4.2.1, `AD-9`). The only clock the crate is allowed to read.
pub trait Clock: 'static {
    /// Wall-clock time, milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
    /// Monotonic milliseconds. MUST never decrease.
    fn monotonic_ms(&self) -> u64;
}

/// Real clock, for hosts (feature `std`).
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct SystemClock { /* an `Instant` captured at construction */ }

#[cfg(feature = "std")]
impl SystemClock { #[must_use] pub fn new() -> Self; }
#[cfg(feature = "std")]
impl Default for SystemClock { /* = new() */ }

/// Deterministic clock for tests (feature `test-util`).
///
/// Interior mutability via `Cell`; the crate is single-threaded by construction (`AD-1`).
#[cfg(feature = "test-util")]
#[derive(Debug)]
pub struct FakeClock { /* Cell<u64> wall, Cell<u64> mono */ }

#[cfg(feature = "test-util")]
impl FakeClock {
    /// Wall clock and monotonic clock both start at `start_ms`.
    #[must_use] pub fn new(start_ms: u64) -> Self;
    /// Advances both clocks by `delta_ms`.
    pub fn advance_ms(&self, delta_ms: u64);
    /// Moves the **wall** clock only; the monotonic clock is untouched, so it never decreases.
    pub fn set_now_ms(&self, value_ms: u64);
}
```

`SystemClock::now_ms` uses `SystemTime::now().duration_since(UNIX_EPOCH)`; a pre-1970 system clock
yields `0` (`unwrap_or_default`) — document that as the invariant, do not `unwrap`.

### 3.6. `src/observe.rs`

```rust
/// Non-fatal conditions worth surfacing to the host (TS `R2.4.2`, `R4.2.9`).
#[non_exhaustive]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WarningCode {
    /// The selected storage backend does not persist across process restarts.
    NonPersistentBackend,
    /// A pre-existing global was replaced because the host allowed it.
    GlobalOverwritten,
}

/// Everything the runtime reports to the host for logging, metrics and tests.
///
/// `#[non_exhaustive]` per `R4.2.11`: later tasks (`T-05`, `T-07`, `T-13`, `T-18`, `T-23`) add
/// variants. Host code matches with a wildcard arm; crate-internal code must not.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ObserverEvent<'a> {
    /// The runtime was installed into a context for this storage key.
    RuntimeInstalled { storage_key: &'a str },
    /// A non-fatal condition.
    Warning { code: WarningCode, detail: &'a str },
    /// The host broke a documented contract (TS `R6.1.2a`, `R8.4.1`).
    HostContractViolation { detail: &'a str },
    /// A storage batch was applied durably.
    StorageBatchApplied { ops: usize, bytes: u64 },
}

/// Observer hook (TS §4.2.5). Implementations MUST NOT affect behaviour.
pub trait SwObserver: 'static {
    fn on_event(&self, event: &ObserverEvent<'_>);
}

/// An observer that discards everything.
#[derive(Copy, Clone, Debug, Default)]
pub struct NullObserver;
impl SwObserver for NullObserver { /* empty body */ }
```

---

## 4. Tests to write

Unit tests live in each module's `#[cfg(test)] mod tests`. One integration file checks the
re-export surface.

| Test | Asserts |
|---|---|
| `error::tests::js_kind_is_total` | A `match` over a `const ALL: [SwError; 21]` array (one value per variant, constructed literally) — **no wildcard arm** — checks `js_kind()` against the §3.2 table. Adding a variant must fail to compile. |
| `error::tests::default_message_table` | Same array; each `default_message()` equals the table's string exactly. |
| `error::tests::dom_name_agrees_with_js_kind` | For every variant: `dom_name().is_some()` iff `js_kind()` is `Dom(_)`, and the names match. |
| `error::tests::host_only_variants_have_no_dom_name` | `ConflictingGlobals`, `AlreadyInstalled`, `Reentrant` → `HostOnly` and `dom_name() == None`. |
| `error::tests::from_storage_error` | Each `StorageError` variant maps per the §3.2 conversion table; `QuotaExceeded` does **not** become `Storage(_)`. |
| `error::tests::from_network_error` | `Timeout → TimedOut`, `Aborted → Aborted`, others → `Network(_)` carrying the `Display` text. |
| `error::tests::from_script_fetch_error` | `BadMime → BadScriptMime`, `Redirected → ScriptRedirect`, `TooLarge`/`TooManyImports → QuotaExceeded`, others → `ScriptFetch(_)`. |
| `error::tests::from_script_eval_error` | All three variants → `ScriptEval(_)` with the `Display` text. |
| `error::tests::display_strings_are_stable` | `to_string()` of each variant matches the `#[error(...)]` template (guards against accidental message edits). |
| `ids::tests::display_format` | `reg#1`, `sw#42`, `job#7`, `dispatch#3`, `cache#9`; `ClientId("abc").to_string() == "abc"`. |
| `ids::tests::allocate_starts_at_one` | First `allocate()` is `1`; `peek()` before/after is `1`/`2`. |
| `ids::tests::restore_from_continues_above_max` | After `restore_from(10)`, `allocate() == 11`; `restore_from(3)` on an allocator already at 12 does **not** move it backwards. |
| `ids::tests::default_matches_new` | `IdAllocator::default().peek() == IdAllocator::new().peek() == 1`. |
| `key::tests::from_origin_serializes` | `https://example.com/a/b?x` → `"https://example.com"`; non-default port preserved (`https://example.com:8443`); `http://EXAMPLE.com` → `"http://example.com"`. |
| `key::tests::from_origin_rejects_opaque` | `data:text/plain,x`, `about:blank` → `Err(SwError::InvalidUrl(_))`. |
| `key::tests::trustworthy_table` | Table test, ≥ 16 cases: `https://x`, `wss://x` → true; `http://localhost`, `http://localhost:8080`, `http://sub.localhost`, `http://LOCALHOST` → true; `http://127.0.0.1`, `http://127.5.5.5`, `http://[::1]` → true; `file:///tmp/a` → true; `http://example.com`, `ws://example.com`, `http://128.0.0.1`, `http://[::2]`, `http://notlocalhost`, `http://localhost.example.com` → false. |
| `key::tests::from_raw_is_verbatim` | `from_raw("weird key")` round-trips through `as_str()` unchanged. |
| `clock::tests::fake_clock_advances_both` | `advance_ms(5)` moves `now_ms` and `monotonic_ms` by 5. |
| `clock::tests::fake_clock_set_now_does_not_move_monotonic` | `set_now_ms(0)` after `new(1000)`: `now_ms() == 0`, `monotonic_ms() == 1000`. |
| `clock::tests::system_clock_monotonic_never_decreases` | 1000 consecutive reads are non-decreasing (feature `std`). |
| `observe::tests::null_observer_is_inert` | Constructing and calling `on_event` for each variant compiles and does nothing. |
| `tests/public_api.rs::reexports_are_reachable` | Uses every re-exported name from §3.1 through `boa_sw_core::…` paths; a missing or renamed export fails to compile. |

**Test-writing rules for this task:** no `proptest` yet (nothing to fuzz here); no snapshot tests;
tests that need `FakeClock` run under `--features test-util`.

---

## 5. Acceptance criteria

Run from the workspace root. Where a criterion needs a feature, the command shows it.

| # | Criterion | Command / check |
|---|---|---|
| 1 | Everything builds, all feature combinations | `cargo build -p boa_sw_core --all-features`, `… --no-default-features`, `… --features test-util`, `… --features serde` |
| 2 | Tests pass | `cargo test -p boa_sw_core --all-features` — every test of §4 present and green |
| 3 | Tests also pass without default features | `cargo test -p boa_sw_core --no-default-features --features test-util` |
| 4 | wasm build unaffected | `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` |
| 5 | **`js_kind` is exhaustive without a wildcard** | `grep -n '_ =>' crates/boa_sw_core/src/error.rs` returns **no line inside `js_kind`/`default_message`**; state this explicitly in the handoff |
| 6 | The mapping test covers all 21 variants | the `ALL` array in `error::tests` has exactly 21 entries; `grep -c` it and say so |
| 7 | Core is still engine-free (`R2.2.2`) | `cargo tree -p boa_sw_core --all-features --edges normal,build,dev \| grep -E 'boa_(engine\|gc\|runtime\|wintertc\|macros)'` → **no output** |
| 8 | Lints | `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` |
| 9 | Formatting | `cargo fmt --all --check` |
| 10 | No forbidden constructs in library code | `grep -nE '\.unwrap\(\)\|\.expect\(\|panic!\|todo!\|unimplemented!' crates/boa_sw_core/src/*.rs` → only lines inside `#[cfg(test)]` blocks; list any hit in the handoff with its justification |
| 11 | No environmental access outside `SystemClock` | `grep -n 'SystemTime\|Instant' crates/boa_sw_core/src/*.rs` → hits only in `clock.rs` inside `#[cfg(feature = "std")]` |
| 12 | Coverage ≥ 95 % for `error.rs` and `key.rs` (`R15.2.1` early target) | `cargo llvm-cov -p boa_sw_core --all-features --summary-only` (install with `cargo install cargo-llvm-cov`). If the tool cannot be installed offline, say so in the handoff and instead list, per public function, the test that exercises it — no function may be uncovered |
| 13 | Public surface matches §3.1 exactly | `cargo doc -p boa_sw_core --all-features --no-deps` builds with no warnings; the integration test of §4 compiles |
| 14 | `ObserverEvent` is `#[non_exhaustive]`, `SwError` is **not** | `grep -B2 'pub enum ObserverEvent' …` shows the attribute; `grep -B2 'pub enum SwError' …` does not |
| 15 | Traceability updated | `docs/traceability.md` gains rows for `R6.1.1`, `R6.1.2`, `R13.1`, `R12.1` (partly), `R4.2.9`, `R4.2.11`, each naming a test from §4 |

---

## 6. Do not do

- No `Registry`, `RegistrationRecord`, `WorkerRecord`, `ScriptResource`, job types, or storage
  traits — that is `T-04`/`T-05`. Only the five modules of §2.
- No extra id types (`TicketId`, `TimerId`, `PortId`, `CacheEntryId`, `JobPromiseId`): the tasks
  that need them define them.
- No `to_js`/`into_js` helper — that lives in the bindings (`T-13`), which consume `js_kind()`.
- No `DOMException` legacy-code table — it belongs to `boa_sw_dom` (`T-11`).
- No changes to `Cargo.toml`, `deny.toml`, `ci.yml`, or any other crate.
- No `#[non_exhaustive]` on `SwError`, and no wildcard arm in `js_kind`/`default_message`.

---

## 7. Handoff

Write `docs/reviews/T-02-handoff.md` with:

1. File-by-file summary of what was added.
2. All 15 acceptance criteria with the command and its result (for criteria 5, 7, 10, 11 quote the
   grep output, including "no output" where that is the expected result).
3. Coverage numbers for `error.rs` and `key.rs`, or the per-function coverage argument if
   `cargo llvm-cov` was unavailable.
4. Deviations — expected: none. Anything else must also appear in `docs/QUESTIONS.md`.
5. `docs/DECISIONS.md` entries added (expected: none; an `I-*` entry only if you were forced into a
   choice this work order did not specify).
6. Requirement ids covered, mirrored into `docs/traceability.md`.
7. Open questions for `T-03` (URL layer), which builds directly on `key.rs` and `error.rs`.

Then stop. `T-03` (URL parsing, scope matching, path restriction) is the next work order.
