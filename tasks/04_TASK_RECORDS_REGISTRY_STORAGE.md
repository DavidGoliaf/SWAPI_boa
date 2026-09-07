# WORK ORDER: T-04 — Records, registry, storage traits

| Metadata | Value |
|---|---|
| **Task id** | `T-04-RECORDS-REGISTRY-STORAGE` |
| **Milestone** | M1 |
| **Target crate** | `crates/boa_sw_core` only |
| **Normative TS** | `TZ_boa_sw_ServiceWorkers.md` §4.2.4, §6.3, §6.4, §6.5, `R6.3.1`, `R6.3.2`, `R15.5.3` |
| **Prerequisites** | accepted `T-03`; `url_util`, ids, storage key, errors and existing core traits are stable |
| **Diff budget** | approximately 900 lines from the TS; hard user limit 3000 changed lines; stop and escalate before exceeding the TS budget by more than 50% |
| **Architect's role** | The public model, storage trait signatures, DTO fields, registry responsibilities and invariant rules are fixed below. |
| **Implementer's role** | Mechanical implementation of this work order only. Do not start T-05 or implement a storage backend. |

---

## 1. Scope And Guardrails

1. **Engine-free core (`AD-2`, `R2.2.2`).** This task must not reference `boa_engine`, `boa_gc`,
   `boa_runtime`, `boa_wintertc`, `JsValue` or any JavaScript object.
2. **Environmental dependencies are traits (`AD-3`).** The storage layer must not open files,
   sockets, spawn threads, read clocks or call host code. `SwStorage` is an interface only.
3. **Atomic storage family (`AD-11`, `R4.2.6`, `R4.2.7`).** Registration/script persistence
   and cache operations use the one `SwStorage` trait. `apply` and `cache_batch` are atomic
   contracts; this task does not implement a backend or simulate transactions in the core.
4. **Explicit errors (`AD-12`).** Reuse `StorageResult`, `StorageError`, `SwError` and existing
   error mappings from T-02. Do not add error variants or invent JavaScript error mappings.
5. **No unsafe code.** `#![deny(unsafe_code)]` remains effective.
6. **No forbidden failure handling.** Library code must contain no `unwrap`, `expect`, `panic!`,
   `todo!` or `unimplemented!`. Tests may use the crate's existing test lint allowance.
7. **No new dependencies.** Use the dependencies already declared by `boa_sw_core`: `url`,
   `indexmap`, `hashbrown`, `smallvec`, `serde` and existing error/ID/key types. A dependency
   change requires an escalation in `docs/QUESTIONS.md` before use.
8. **Serde is feature-gated.** `Serialize`/`Deserialize` derives are enabled only when the
   existing `serde` feature is active. The default and `--no-default-features` builds must not
   require serde derives.
9. **State ownership.** `Registry` must not expose mutable public fields. Mutations, slot
   changes and worker insertion/removal go through methods so later lifecycle tasks can preserve
   invariants.
10. **State transition boundary (`R6.3.2`).** T-04 defines record storage and invariant checks;
    it does not implement `SwCore::update_worker_state` or lifecycle notification commands.
    No direct-state-transition API should be added as a substitute for T-05.
11. **Resolved T-03 questions.** Q-02 and Q-03 are closed decisions, not implementation choices;
    the exact consequences are repeated in §3.3 and §4.2 below.
12. **Storage DTO contract.** The TS names all storage DTOs but leaves some field layouts implicit.
    This work order fixes those layouts in §5.2–§5.3. The implementer must not add alternate
    variants, wrappers or backend-specific fields.

---

## 2. Files To Create Or Modify

```text
crates/boa_sw_core/src/model.rs       # Worker/registration/resource records and enums.
crates/boa_sw_core/src/registry.rs    # Encapsulated registration/worker indexes and lookups.
crates/boa_sw_core/src/storage.rs     # SwStorage trait and persistence/cache DTOs.
crates/boa_sw_core/src/invariants.rs  # Registry::check_invariants implementation.
crates/boa_sw_core/src/lib.rs         # Register modules and only required public exports.
docs/traceability.md                  # Add T-04 requirement-to-test rows.
docs/reviews/T-04-handoff.md          # Required implementation handoff.
```

No other source crate, manifest, CI file, backend, job queue, core event machine or cache
algorithm may be changed. `docs/QUESTIONS.md` may be changed only when an actual ambiguity or
conflict is found and the implementation cannot proceed mechanically.

---

## 3. Normative Model API

### 3.1. Enums

Implement these public enums in `model.rs` with the listed variants and public documentation:

```rust
pub enum WorkerState {
    Parsed,
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

pub enum WorkerType {
    Classic,
    Module,
}

pub enum UpdateViaCache {
    Imports,
    All,
    None,
}

pub enum RunState {
    NotRunning,
    Starting,
    Running,
    Terminating,
}
```

`UpdateViaCache::Imports` is the default mode. The exact enum variants are part of the public
contract and must not be renamed or expanded in this task.

`EventType` must represent the event types needed by §6.3's `handled_event_types` field and later
event tasks. Implement exactly these variants: `Install`, `Activate`, `Fetch`, `Message`, `Push`,
`Sync`, `NotificationClick` and `NotificationClose`. No payloads or JavaScript values are stored
in the enum. Later tasks may record these values but may not rename them.

All model enums must derive `Clone`, `Copy` where semantically appropriate, `Debug`, `PartialEq`
and `Eq`. Add `Serialize`/`Deserialize` only behind the existing `serde` feature.

### 3.2. Worker and registration records

Implement the following fields exactly as specified by TS §6.3:

```rust
pub struct WorkerRecord {
    pub id: WorkerId,
    pub registration: RegistrationId,
    pub script_url: Url,
    pub worker_type: WorkerType,
    pub state: WorkerState,
    pub skip_waiting: bool,
    pub imported_scripts_updated: bool,
    pub has_fetch_handler: Option<bool>,
    pub handled_event_types: SmallVec<[EventType; 8]>,
    pub run_state: RunState,
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
    pub navigation_preload_header: String,
    pub uninstalling: bool,
}
```

Defaults required by the TS:

- `UpdateViaCache` defaults to `Imports`.
- `navigation_preload_enabled` defaults to `false`.
- `navigation_preload_header` defaults to `"true"`.
- Newly created records start with no worker slots, `uninstalling == false`,
  `run_state == NotRunning`, `pending_events == 0`, `skip_waiting == false`,
  `imported_scripts_updated == false`, and `has_fetch_handler == None`.
- `scope` is stored normalized without a fragment; its query is preserved, per `R6.1.3`.

`WorkerRecord` and `RegistrationRecord` must derive `Clone` and `Debug`, and must derive
`Serialize`/`Deserialize` with the `serde` feature.

### 3.3. Script resources

Implement the §6.4 types exactly:

```rust
pub struct ScriptResource {
    pub url: Url,
    pub bytes: Rc<[u8]>,
    pub sha256: [u8; 32],
    pub mime: String,
    pub service_worker_allowed: Option<String>,
    pub is_main: bool,
}

pub type ScriptResourceMap = IndexMap<Url, ScriptResource>;
```

`mime` is the already-lowercased MIME essence. T-04 does not validate MIME, fetch scripts,
compute hashes or compare bytes; those behaviours belong to later lifecycle tasks. The resource
map must preserve `IndexMap` insertion order and use absolute `Url` keys as required by
`R6.4.5`. Resource types must derive `Clone` and `Debug`, plus serde derives behind the feature.

### 3.4. Resolved T-03 decisions used by T-04

- **Q-02 / scope fragments:** `url_util::scope_matches` strips the fragment from both the scope
  and client serialized URLs before applying the prefix comparison. T-04 must pass stored scope
  URLs directly to `scope_matches`; it must not strip fragments a second time, serialize either
  URL into a new `String`, or implement a path-segment boundary check. Registry insertion must
  still normalize stored scopes to fragment-free URLs as required by `R6.1.3`.
- **Q-03 / malformed `Service-Worker-Allowed`:** this is outside `Registry` and storage. If a
  later caller invokes `path_restriction_ok` with malformed `allowed`, the helper treats it as
  absent and applies the default script-directory restriction. It returns `PathRestriction` only
  when that restriction fails; it never converts this case into `InvalidUrl`. T-04 must not add
  another parser, error variant, or registry-level special case.

---

## 4. Registry Contract

### 4.1. Internal indexes

Implement a private `Registry` with these indexes and ownership relationships:

- `IndexMap<(StorageKey, String), RegistrationId>` for registration lookup by storage key and
  serialized scope. The serialized scope is fragment-free and preserves its query.
- `HashMap<RegistrationId, RegistrationRecord>` for registration records.
- `HashMap<WorkerId, WorkerRecord>` for worker records.

The fields must not be `pub`. The registry must not expose a mutable iterator or mutable map.
Read-only accessors may return references or copies as needed by the specified lookup methods.

### 4.2. Required operations

The registry must provide methods sufficient for the following public `SwCore` lookup contract
from §6.5 and for later lifecycle tasks:

```rust
impl SwCore {
    pub fn match_registration(
        &self,
        key: &StorageKey,
        client_url: &Url,
    ) -> Option<RegistrationId>;

    pub fn get_registration(
        &self,
        key: &StorageKey,
        scope: &Url,
    ) -> Option<RegistrationId>;

    pub fn newest_worker(&self, reg: RegistrationId) -> Option<WorkerId>;

    pub fn registrations_for_origin(
        &self,
        key: &StorageKey,
    ) -> Vec<RegistrationId>;
}
```

The exact `Registry` method surface for T-04 is:

```rust
impl Registry {
    pub fn new() -> Self;
    pub fn insert_registration(&mut self, record: RegistrationRecord) -> Result<(), String>;
    pub fn insert_worker(&mut self, record: WorkerRecord) -> Result<(), String>;
    pub fn replace_registration(&mut self, record: RegistrationRecord) -> Result<(), String>;
    pub fn replace_worker(&mut self, record: WorkerRecord) -> Result<(), String>;
    pub fn remove_registration(
        &mut self,
        id: RegistrationId,
    ) -> Result<Option<RegistrationRecord>, String>;
    pub fn remove_worker(&mut self, id: WorkerId) -> Result<Option<WorkerRecord>, String>;
    pub fn registration(&self, id: RegistrationId) -> Option<&RegistrationRecord>;
    pub fn worker(&self, id: WorkerId) -> Option<&WorkerRecord>;
    pub fn get_registration(&self, key: &StorageKey, scope: &Url)
        -> Option<RegistrationId>;
    pub fn match_registration(&self, key: &StorageKey, client_url: &Url)
        -> Option<RegistrationId>;
    pub fn newest_worker(&self, reg: RegistrationId) -> Option<WorkerId>;
    pub fn registrations_for_origin(&self, key: &StorageKey) -> Vec<RegistrationId>;
}
```

`insert_registration` strips a fragment from the stored scope before indexing it. It returns
`Err` for a duplicate registration ID or `(StorageKey, serialized scope)` key. `insert_worker`
returns `Err` for a duplicate worker ID or a missing registration. `remove_registration` returns
`Err` if any worker slot is still occupied; after slots are cleared it removes the registration
and its scope index entry. `remove_worker` returns `Err` if the worker is still referenced by a
registration slot; otherwise it removes the worker record. Missing IDs return `Ok(None)` from the
remove methods. `replace_registration` and `replace_worker` preserve the existing ID and index
identity, validate the referenced registration/slots, and atomically replace the record. They are
the only methods later lifecycle code may use to mutate slots or worker fields. These error strings
are host/internal diagnostics and are not `SwError` values.

The methods above must not add a `SwCore` type or implement the T-05 core machine merely to
satisfy this task. The required lookup behaviour is:

1. `get_registration` is isolated by `StorageKey` and uses the normalized serialized scope key.
2. `match_registration` is isolated by `StorageKey`, calls
   `url_util::scope_matches`, ignores `uninstalling` records, and returns the registration with
   the longest matching serialized scope. The intentional `/foo` versus `/foobar` prefix rule
   from `R6.2.2` remains in force.
3. `newest_worker` returns `installing`, otherwise `waiting`, otherwise `active`.
4. `registrations_for_origin` returns only records for the requested key, in registry insertion
   order. It must not expose records under another storage key.
5. Insert and remove helpers maintain both indexes and reject or safely handle duplicate/missing
   IDs without panicking. A removed registration must not remain in the scope index; removed
   workers must not remain reachable from worker indexes or registration slots.

The registry must not implement job queues, state transition notifications, persistence calls,
HTTP, client matching outside registration scope lookup, or lifecycle algorithms.

---

## 5. Storage Trait And Data Types

### 5.1. Required trait signatures

Implement `SwStorage` in `storage.rs` with the exact §4.2.4 signatures. Reuse the existing
`StorageResult` type from T-02 and the existing ID/key types:

```rust
pub trait SwStorage: 'static {
    fn load_registrations(
        &self,
        key: &StorageKey,
    ) -> StorageResult<Vec<PersistedRegistration>>;

    fn apply(&self, key: &StorageKey, batch: StorageBatch) -> StorageResult<()>;

    fn load_script_map(&self, worker: WorkerId) -> StorageResult<ScriptResourceMap>;

    fn cache_list(&self, key: &StorageKey) -> StorageResult<Vec<CacheName>>;

    fn cache_open(
        &self,
        key: &StorageKey,
        name: &CacheName,
        create: bool,
    ) -> StorageResult<Option<CacheId>>;

    fn cache_delete(&self, key: &StorageKey, name: &CacheName)
        -> StorageResult<bool>;

    fn cache_query(
        &self,
        cache: CacheId,
        query: &CacheQuery,
    ) -> StorageResult<Vec<CacheEntryId>>;

    fn cache_read(
        &self,
        cache: CacheId,
        entry: CacheEntryId,
    ) -> StorageResult<CacheEntry>;

    fn cache_keys(&self, cache: CacheId) -> StorageResult<Vec<CacheEntryId>>;

    fn cache_batch(
        &self,
        cache: CacheId,
        ops: Vec<CacheOperation>,
    ) -> StorageResult<CacheBatchReport>;

    fn usage(&self, key: &StorageKey) -> StorageResult<u64>;
}
```

The trait is an interface only. T-04 must not add an implementation for memory, SQLite or any
other backend. Methods must not call back into runtime code; atomicity and re-entrancy are backend
contracts recorded by `R4.2.6`–`R4.2.8`.

### 5.2. Persistence DTOs

Define the named DTOs required by the trait and by `StorageBatch` with these exact layouts:

- `PersistedRegistration` containing `registration: RegistrationRecord` and
  `workers: Vec<WorkerRecord>`. Script maps are intentionally not embedded because
  `load_script_map(worker)` loads them separately.
- `StorageBatch`, preserving an ordered `Vec<StorageOp>`; the storage key remains the separate
  `SwStorage::apply` argument from §5.1.
- `StorageOp` with exactly these typed variants:
  - `PutRegistration(RegistrationRecord)`
  - `DeleteRegistration(RegistrationId)`
  - `PutWorker(WorkerRecord)`
  - `DeleteWorker(WorkerId)`
  - `PutScript { worker: WorkerId, resource: ScriptResource }`
  - `DeleteScriptsOf(WorkerId)`
- `StorageResult<T>` must remain the existing T-02 alias/type and must not be redefined in a way
  that changes its public error mapping.

`StorageBatch` contains `pub ops: Vec<StorageOp>` and provides a constructor/accessor only as
needed by the implementation; no backend handle or storage key is stored in it. The operation
payloads are typed and carry enough data for a backend to round-trip registrations, workers and
script resources without serializing opaque debug text. Preserve operation order; later backends
depend on it for atomic application and recovery.

### 5.3. Cache DTOs

Define the named DTOs required by §4.2.4 and §9.1–§9.2 with these exact layouts. These are data
contracts only; T-04 does not implement their algorithms.

- `CacheQuery`, with `request: Option<CacheRequestKey>`, `ignore_search`, `ignore_method` and
  `ignore_vary` exactly as §9.2.
- `CacheOperation` with exactly `Put(CacheEntry)` and `Delete(CacheEntryId)` variants.
- `CacheEntry` with fields `id: CacheEntryId`, `request: CacheRequestKey`,
  `response: CacheResponse`, `vary: Vec<HeaderName>`, `insertion_order: u64` and
  `byte_size: u64`.
- `CacheResponse` with fields `kind: CacheResponseKind`, `url_list: Vec<Url>`, `status: u16`,
  `status_text: String`, `headers: HeaderMap`, `body: Rc<Vec<u8>>` and `redirected: bool`.
- `CacheResponseKind` with exactly `Basic`, `Cors`, `Default`, `Error`, `Opaque` and
  `OpaqueRedirect` variants.
- `CacheEntryId(pub u64)` as a typed, opaque cache-entry identifier. It must provide the same
  `from_raw`/`raw` pattern as the existing ID newtypes but is not interchangeable with `CacheId`.
- `CacheName(pub String)` as the owned cache-name value. Cache names are arbitrary DOM strings;
  T-04 performs no policy validation beyond storing the value.
- `CacheBatchReport` with fields `removed: u32`, `inserted: u32` and `usage_bytes: u64`.
- `CacheRequestKey` with fields `method: Method`, `url: Url` and `headers: HeaderMap`. Bodies
  are intentionally excluded because cache lookup keys are body-less.

`CacheQuery` has fields `pub request: Option<CacheRequestKey>`, `pub ignore_search: bool`,
`pub ignore_method: bool` and `pub ignore_vary: bool`. Use `http::{HeaderMap, HeaderName, Method}`
for the HTTP fields. All DTOs are engine-free and derive `Clone` and `Debug`; add serde derives
behind the existing feature. Do not implement query matching, Vary matching, quota planning,
cloning, cache algorithms or backend persistence in T-04; those belong to T-08–T-10.

---

## 6. Invariants

Implement `Registry::check_invariants(&self) -> Result<(), String>` in `invariants.rs`.
It may be available in all builds for testability; it must not panic and must return a useful
diagnostic identifying the violated invariant. It must check every rule in `R15.5.3`:

1. A worker ID is not present in more than one of `installing`, `waiting` or `active` slots of
   the same or different registrations.
2. A registration with no workers is permitted only when `uninstalling == true`.
3. A worker with `run_state == Running` is not in `WorkerState::Redundant`.
4. `pending_events` cannot become negative. Use the unsigned field and mutation methods to make
   underflow impossible; tests must cover attempted invalid decrement if such a method exists.
5. Registration IDs and worker IDs are unique across their respective indexes, and every worker
   referenced by a registration slot exists in the worker index.
6. Every indexed registration has a matching record whose `id`, `storage_key` and normalized
   scope agree with the index key.
7. Every worker record points to an existing registration.

The checker must not mutate the registry. Do not add `unsafe` debug-only shortcuts.

---

## 7. Tests To Write

All tests should be deterministic unit tests in the three relevant modules unless an integration
test is necessary to verify public exports. Test helpers may construct deliberately broken
registries inside the crate's test module; production fields remain private.

| Test | Fixture and assertions |
|---|---|
| `model::tests::defaults_are_spec_values` | Construct default/empty records and assert `Imports`, disabled navigation preload, header `"true"`, empty slots, `NotRunning`, zero pending events and unset fetch-handler state. |
| `model::tests::script_resource_map_preserves_absolute_url_keys` | Insert two absolute resources into `IndexMap`; assert key lookup, `is_main`, MIME, bytes, hash and insertion order. |
| `model::tests::serde_round_trip_records` | With `serde`, round-trip every model record and enum, including `Rc<[u8]>`, URLs, optional fields and `SmallVec`; assert equality. |
| `registry::tests::get_registration_isolated_by_key` | Same scope under two `StorageKey`s; each lookup returns only its own registration. |
| `registry::tests::match_registration_longest_prefix_wins` | Three overlapping scopes; assert the longest matching serialized scope wins, including the intentional `/foo` to `/foobar` prefix behavior. |
| `registry::tests::match_registration_skips_uninstalling` | Make the longest matching registration uninstalling; assert the next eligible registration is selected, or `None` if none remains. |
| `registry::tests::newest_worker_precedence` | Populate all three worker slots and assert installing, then waiting, then active precedence as slots are cleared. |
| `registry::tests::registrations_preserve_insertion_order` | Insert registrations under one key and assert `registrations_for_origin` order and key isolation. |
| `registry::tests::replace_methods_preserve_indexes` | Replace registration slots and worker state through methods; assert index identity remains stable and invalid replacements are rejected without partial mutation. |
| `registry::tests::remove_cleans_all_indexes` | Remove a registration and worker; assert lookups, slots and indexes no longer expose them and the invariants checker passes. |
| `invariants::tests::detects_worker_in_two_slots` | Deliberately reference one worker in two slots; assert `Err` with a useful diagnostic. |
| `invariants::tests::detects_empty_non_uninstalling_registration` | Registration has no installing/waiting/active worker and is not uninstalling; assert failure. |
| `invariants::tests::detects_running_redundant_worker` | Set a worker to `Running` plus `Redundant`; assert failure. |
| `invariants::tests::detects_duplicate_or_dangling_ids` | Construct duplicate/dangling slot/index relationships; assert failure. |
| `invariants::tests::detects_registration_index_mismatch` | Corrupt serialized scope/key or storage key relationship; assert failure. |
| `invariants::tests::detects_worker_registration_mismatch` | Worker points to a missing registration; assert failure. |
| `storage::tests::storage_types_are_typed_and_ordered` | Build every `StorageOp` variant in one ordered batch; assert order and payloads are retained without stringly typed serialization. |
| `storage::tests::serde_round_trip_storage_types` | With `serde`, round-trip every public storage DTO, every storage operation and a representative script/cache entry. |
| `storage::tests::trait_signature_smoke` | Define a minimal test-only `SwStorage` implementation and compile calls to every trait method; no backend behavior is required. |
| `public_api::tests::t04_exports_are_reachable` | Verify only the intended model, registry/storage types and `SwStorage` are reachable from `boa_sw_core`; no mutable registry fields are public. |

The invariant suite must contain at least one dedicated failing-case test for each rule in §6,
not one aggregate test that happens to cover several rules. Tests must assert both failure and
that valid registries pass.

---

## 8. Acceptance Criteria

Run commands from the workspace root. The handoff must record the exact command and result for
every item below.

1. **Scope and budget:** only the files listed in §2 plus allowed documentation files are changed;
   implementation changes remain below 3000 lines and preferably within the TS budget of about
   900 lines. Verify with `git diff --stat` and `git diff --name-only`.
2. **Model API:** all enums, records and script-resource types from §3 exist with the exact
   field names/types, derive `Clone`/`Debug` as required, and compile with and without serde.
3. **Registry encapsulation:** `Registry` has no public mutable fields; all index/slot mutation
   goes through methods. Verify by the public API test and source inspection.
4. **Registry behavior:** longest-prefix matching, storage-key isolation, uninstalling skip,
   newest-worker precedence, insertion order and index cleanup all have dedicated passing tests.
5. **Storage API:** `SwStorage` matches every §5.1 signature exactly, all named DTOs exist, and
   no backend or cache algorithm is implemented. Verify with the trait smoke test and source
   inspection.
6. **Invariant coverage:** every `R15.5.3` invariant has a dedicated failing-case test and a
   valid-registry success assertion. `Registry::check_invariants` never panics.
7. **Serde feature:** `cargo test -p boa_sw_core --features serde,test-util` passes, including
   round trips for every model and storage DTO required by this task.
8. **Feature matrix:** all of these succeed:
   `cargo build -p boa_sw_core --all-features`,
   `cargo build -p boa_sw_core --no-default-features`, and
   `cargo build -p boa_sw_core --features test-util`.
9. **Tests:** `cargo test -p boa_sw_core --all-features` passes, including all existing T-01–T-03
   tests and every test listed in §7.
10. **Coverage:** `cargo llvm-cov -p boa_sw_core --all-features --summary-only` reports at least
    92% coverage for `model.rs`, `registry.rs` and `storage.rs` together, with the exact output
    recorded in the handoff. If the tool is unavailable, stop and document the limitation rather
    than claiming a measurement.
11. **Wasm:** `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features`
    succeeds.
12. **Engine-free core:**
    `cargo tree -p boa_sw_core --all-features --edges normal,build,dev | grep -E 'boa_(engine|gc|runtime|wintertc|macros)'`
    produces no output.
13. **Lints and formatting:**
    `cargo fmt --all --check` and
    `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` pass.
14. **Dependency/license gate:** `cargo deny check` passes. No new dependency or manifest change
    is present.
15. **Forbidden constructs:** source inspection confirms no `unsafe`, `panic!`, `todo!`,
    `unimplemented!`, `unwrap` or `expect` in production code added by T-04.
16. **Traceability:** `docs/traceability.md` contains rows for `R4.2.6`–`R4.2.8`,
    `R6.1.3`, `R6.3.1`, `R6.3.2`, `R6.4.5`, `R15.5.3` and each row names a concrete test.
17. **Resolved-question conformance:** the registry tests demonstrate direct use of
    `url_util::scope_matches` with the Q-02 fragment semantics, and no T-04 code introduces a
    second `Service-Worker-Allowed` parser or changes the Q-03 error behavior.
18. **Handoff:** `docs/reviews/T-04-handoff.md` contains the file summary, all acceptance
    commands/results, coverage numbers, deviations/questions, decisions, covered requirements,
    and open questions for T-05.

---

## 9. Do Not Do

- Do not implement `SwCore`, `CoreEvent`, `CoreCommand`, job queues or lifecycle algorithms; T-05
  owns those.
- Do not implement `MemoryStorage`, `SqliteStorage`, cache matching, cache batch planning or
  quota enforcement; those belong to T-08–T-10.
- Do not implement HTTP fetching, script validation, byte comparison, update-via-cache behavior,
  install/activate/unregister, worker state notifications or recovery.
- Do not add `CacheId` replacements, new ID allocators, new `SwError` variants or new public
  architectural abstractions.
- Do not modify other crates, manifests, CI, `deny.toml`, existing T-02/T-03 behavior or the
  pinned specification.
- Do not exceed 3000 changed lines. Do not add alternate storage DTO layouts, operation variants,
  registry mutation semantics or error types beyond the contracts in this work order.

---

## 10. Handoff Requirements

Write `docs/reviews/T-04-handoff.md` and stop after T-04. It must include:

1. File-by-file implementation summary and changed-line count.
2. Exact results for all 18 acceptance criteria, including the expected no-output dependency
   command and the measured module coverage.
3. The complete registry lookup and invariant test names, with the invariant-to-test mapping.
4. Any deviations from this work order, mirrored in `docs/QUESTIONS.md`; expected result is none.
5. Any `docs/DECISIONS.md` entry added; expected result is none unless a dependency or API
   decision was escalated and answered.
6. Requirement IDs mirrored into `docs/traceability.md`.
7. Open questions and integration notes for T-05, especially how its `SwCore` will use registry
   mutation methods and `SwStorage::apply`.

Then stop. T-05 is the next work order.
