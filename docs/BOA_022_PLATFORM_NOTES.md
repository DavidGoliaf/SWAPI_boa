# Boa 0.22 platform notes for `boa-sw`

| Metadata | Value |
|---|---|
| Purpose | Record what the Boa 0.22 platform actually provides, so implementers do not guess |
| Engine | `boa_engine` / `boa_gc` / `boa_runtime` / `boa_wintertc` 0.22.0, edition 2024, MSRV 1.91.0 |
| Verified against | the `boa` checkout at `../boa` (workspace version `0.22.0`) |
| Status | normative for implementation choices; update it when you discover something new |

Paths below are relative to the Boa repository checkout.

---

## 1. Realms — the basis of `AD-1`

`core/engine/src/context/mod.rs`:

```rust
// Create a new Realm with the default global bindings.
pub fn create_realm(&mut self) -> JsResult<Realm>;
// Replaces the currently active realm with `realm`, returns the old one.
pub fn enter_realm(&mut self, realm: Realm) -> Realm;
pub fn realm(&self) -> &Realm;
```

`core/engine/src/realm.rs` gives per-realm state: `intrinsics()`, `host_defined()`,
`host_defined_mut()`, `register_class::<C>()`, `has_class::<C>()`, `scope()`.

Consequences:

* One `Context` can host the client realm and any number of service-worker realms.
* Each realm has its own global object, its own intrinsics and its own class registry, which is
  exactly what `ServiceWorkerGlobalScope` needs.
* `enter_realm` swaps the realm of the **current call frame**, so it must be restored on every
  path — hence the mandatory `with_realm` helper (TS `R3.4.3`).
* Dropping the `Realm` handle plus every object referencing it lets the GC reclaim the worker.

## 2. Jobs and the event loop

`core/engine/src/job.rs`:

```rust
pub enum Job { PromiseJob(..), AsyncJob(NativeAsyncJob), TimeoutJob(..), IntervalJob(..),
               GenericJob(..), /* FinalizationRegistry cleanup */ }

impl NativeAsyncJob {
    pub fn new<F>(f: F) -> Self where F: AsyncFnOnce(&RefCell<&mut Context>) -> JsResult<JsValue> + 'static;
    pub fn with_realm<F>(f: F, realm: Realm) -> Self where F: /* same */;
}

pub trait JobExecutor: Any {
    fn enqueue_job(self: Rc<Self>, job: Job, context: &mut Context);
    fn run_jobs(self: Rc<Self>, context: &mut Context) -> JsResult<()>;
    async fn run_jobs_async(self: Rc<Self>, context: &RefCell<&mut Context>) -> JsResult<()>;
}
```

Consequences:

* `NativeAsyncJob::with_realm` is how a worker's promise reactions get the right realm — use it.
* `boa-sw` does **not** install its own `JobExecutor`; the host owns it (TS `R11.1.3`).
* Async jobs receive `&RefCell<&mut Context>`; never hold a `SwRuntime` borrow across an `.await`.

## 3. Structured clone — `JsValueStore`

`core/wintertc/src/store/mod.rs`:

```rust
pub struct JsValueStore(Arc<ValueStoreInner>);
impl JsValueStore {
    pub fn try_from_js(value: &JsValue, context: &mut Context, transfer: Vec<JsValue>) -> JsResult<Self>;
}
impl TryIntoJs for JsValueStore { /* rebuilds the value in any Context/realm */ }
```

* Context-free, `Send`, cycle-safe, supports transfer lists (`is_transferable` gate).
* This is the serializer for every `postMessage` path (TS `AD-6`, `R10.3.2`) when the
  `runtime-interop` feature is on.
* Re-exported through `boa_runtime` as the backing store of `structuredClone`.

## 4. What `boa_runtime` / `boa_wintertc` already provide

| Module | Provides | Use it? |
|---|---|---|
| `boa_wintertc::url` / `boa_runtime::url` | `URL`, `URLSearchParams` (feature `url`) | Yes — never re-implement |
| `boa_runtime::text` | `TextEncoder`, `TextDecoder` | Yes |
| `boa_wintertc::clone` | `structuredClone` | Yes |
| `boa_wintertc::base64` | `atob`, `btoa` | Yes |
| `boa_wintertc::timers` | `setTimeout`/`setInterval`/clear | Yes (host registers them) |
| `boa_wintertc::microtask` | `queueMicrotask` | Yes |
| `boa_wintertc::abort` | `JsAbortSignal` (used by `fetch`) | Yes, or the `boa_sw_dom` version when `dom-shim` is on |
| `boa_runtime::console` | `console` | Host's choice |
| `boa_runtime::message` | `MessageSender` trait + `postMessage` helper for worker-style hosts | Reference only; `boa-sw` has its own routing |

**`boa_wintertc::events` is a stub.** `core/wintertc/src/events/mod.rs` is 29 lines with a
`register()` that returns `Ok(())` and a TODO list. There is **no** `EventTarget`, `Event`,
`CustomEvent`, `ErrorEvent` or `MessageEvent` in the engine. `boa_sw_dom` (task T-11) must
implement them.

## 5. `boa_runtime::fetch` — why `boa-sw` owns its own Fetch classes

`core/runtime/src/fetch/`:

```rust
pub trait Fetcher: NativeObject {
    fn resolve_uri(&self, uri: String, ctx: &mut Context) -> JsResult<String> { Ok(uri) }
    async fn fetch(self: Rc<Self>, request: JsRequest, signal: Option<JsObject>,
                   context: &RefCell<&mut Context>) -> JsResult<JsResponse>;
}

pub struct JsRequest  { inner: http::Request<Vec<u8>>, signal: Option<JsObject> }   // #[boa_class(rename = "Request")]
pub struct JsResponse { url: JsString, type_: ResponseType, status: u16,
                        status_text: JsString, headers: JsHeaders, body: Rc<Vec<u8>> }
```

What is missing for Service Workers:

* `Request` has no `mode`, `credentials`, `destination`, `cache`, `redirect` — all of which the
  Spec exposes on `FetchEvent.request` and uses in `respondWith` validation.
* `JsResponse` cannot be constructed with a chosen `type`/`url`/`redirected` (only `basic()` and
  `error()`), and does not expose `url`/`type` to Rust — so a Cache round-trip would lose them.
* Bodies are already fully buffered `Vec<u8>`, which matches TS `AD-7`; no loss there.

Therefore `boa_sw_fetch` implements its own `Headers`/`Request`/`Response` (TS `AD-8`) and offers
`runtime-interop` adapters (`SwRequest ⇄ JsRequest`, `impl HttpClient for RuntimeFetcherClient<T>`)
so a host that already has a `Fetcher` can plug it in. A host must not register both class sets in
the same realm; `boa_sw` detects the conflict at registration time.

## 6. Object model, classes and GC

* `#[derive(Trace, Finalize, JsData)]` on native structs; `#[unsafe_ignore_trace]` only for
  types with no GC-managed content (numeric ids, enums, `Url`, `HeaderMap`, `Vec<u8>`).
* `JsObject::from_proto_and_data(proto, data)`, `obj.downcast_ref::<T>() -> Option<GcRef<T>>`
  (note: `Option`, not `Result`), `obj.set_prototype(proto)`.
* `#[boa_class]` / `#[boa_module]` macros are re-exported from `boa_engine` (`core/engine/src/lib.rs`),
  together with `TryFromJs` / `TryIntoJs` derives — prefer them over hand-written builders.
* `boa_gc::WeakGc` is available for the per-realm identity maps (TS `R3.3.4`).

## 7. Value and string handling (carry-over from `boa-idb`)

`boa-idb`'s `docs/BOA_022_INCOMPATIBILITIES.md` and `docs/BOA_022_INTEGRATION_GUIDE.md` document
fourteen API shapes that surprised implementers. The ones that matter here:

* `JsValue` is NaN-boxed: there are no public enum variants. Use `as_number()`, `as_string()`,
  `as_object()`, `as_boolean()`, `as_bigint()`, `as_symbol()`, `is_null()`, `is_undefined()` —
  these are allocation-free and need no `Context`. `to_number`/`to_string` perform JS coercion.
* `JsString` is UTF-16: use `to_vec() -> Vec<u16>`, `code_unit_at`, `JsString::from(&[u16])`.
  **`to_std_string_escaped()` loses unpaired surrogates** — never use it for URLs, header values,
  cache keys or message payloads. Use `to_std_string()` and handle its error where UTF-8 is required
  (e.g. `TypeError` for URLs with unpaired surrogates, as `boa_runtime::fetch` does).
* `has_property`, `get_own_property` and friends take `&mut Context`.
* `Copy` cannot be derived together with `Trace`; use `Clone`.
* `JsPromise`: `new_pending(ctx) -> (JsPromise, ResolvingFunctions)`, `then`, `catch`, `finally`,
  `from_async_fn`, `into_js_future`, `state()`. `new_pending` + stored `ResolvingFunctions` is the
  pattern for job promises, tickets and `preloadResponse`.

## 8. Open items to re-check when Boa updates

1. An interrupt/step-budget facility (needed for real worker termination, TS `AD-10`, risk R1).
2. `boa_wintertc::events` landing upstream — if it does, `boa_sw_dom` should become a thin adapter.
3. Streams support — would unlock `Response.body` and streaming `respondWith` (TS §1.4, risk R3).
4. A public `JsResponse` constructor with `type`/`url`/`redirected` — would reduce the amount of
   duplication between `boa_sw_fetch` and `boa_runtime::fetch`.
