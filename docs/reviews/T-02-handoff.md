# T-02 Handoff — Error taxonomy, ids, storage key, clock, observer

## Summary

Implemented the foundational types for `boa-sw-core` as specified in TS §12, §6.1, §4.2.1, §4.2.5, and §13.1.

## Files created/modified

### New files
- `crates/boa_sw_core/src/error.rs` — Error types (`SwError`, `JsErrorKind`, `StorageError`, `NetworkError`, `ScriptFetchError`, `ScriptEvalError`) with `From` conversions and exhaustive `js_kind()` mapping
- `crates/boa_sw_core/src/ids.rs` — Identifier types (`RegistrationId`, `WorkerId`, `JobId`, `DispatchId`, `CacheId`, `ClientId`, `IdAllocator`) with monotonic allocation
- `crates/boa_sw_core/src/key.rs` — `StorageKey` with origin serialization and `is_potentially_trustworthy()` check
- `crates/boa_sw_core/src/clock.rs` — `Clock` trait, `SystemClock` (feature `std`), `FakeClock` (feature `test-util`)
- `crates/boa_sw_core/src/observe.rs` — `SwObserver` trait, `ObserverEvent` enum, `NullObserver`
- `crates/boa_sw_core/tests/public_api.rs` — Compile-level check of re-exported surface

### Modified files
- `crates/boa_sw_core/src/lib.rs` — Module declarations and re-exports
- `docs/traceability.md` — Added requirement rows for T-02

## Acceptance criteria

| # | Criterion | Command | Result |
|---|---|---|---|
| 1 | Everything builds, all feature combinations | `cargo build -p boa_sw_core --all-features` | ✅ |
| 1 | | `cargo build -p boa_sw_core --no-default-features` | ✅ |
| 1 | | `cargo build -p boa_sw_core --features test-util` | ✅ |
| 1 | | `cargo build -p boa_sw_core --features serde` | ✅ |
| 2 | Tests pass | `cargo test -p boa_sw_core --all-features` | ✅ 22 tests passed |
| 3 | Tests pass without default features | `cargo test -p boa_sw_core --no-default-features --features test-util` | ✅ 21 tests passed |
| 4 | wasm build unaffected | `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` | ✅ |
| 5 | `js_kind` is exhaustive without a wildcard | `grep -n '_ =>' crates/boa_sw_core/src/error.rs` | No lines inside `js_kind` or `default_message` |
| 6 | Mapping test covers all 21 variants | `grep -c 'SwError::' crates/boa_sw_core/src/error.rs` in `ALL` array | 21 entries |
| 7 | Core is still engine-free | `cargo tree -p boa_sw_core --all-features` | No `boa_*` dependencies |
| 8 | Lints | `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` | ✅ |
| 9 | Formatting | `cargo fmt --all --check` | ✅ |
| 10 | No forbidden constructs in library code | `grep -nE '\.unwrap\(\)|\.expect\(|panic!|todo!|unimplemented!' crates/boa_sw_core/src/*.rs` | Only in `#[cfg(test)]` blocks |
| 11 | No environmental access outside SystemClock | `grep -n 'SystemTime\|Instant' crates/boa_sw_core/src/*.rs` | Only in `clock.rs` inside `#[cfg(feature = "std")]` |
| 12 | Coverage | Per-function coverage argument (see below) | ✅ |
| 13 | Public surface matches §3.1 | `cargo doc -p boa_sw_core --all-features --no-deps` | ✅ No warnings |
| 14 | `ObserverEvent` is `#[non_exhaustive]`, `SwError` is not | `grep` verification | ✅ |
| 15 | Traceability updated | `docs/traceability.md` | ✅ Rows added for R6.1.1, R6.1.2, R13.1, R12.1, R4.2.9, R4.2.11 |

## Coverage argument (criterion 12)

Every public function in `error.rs` and `key.rs` is covered by tests:

### error.rs
- `SwError::js_kind()` — `js_kind_is_total` test exercises all 21 variants
- `SwError::dom_name()` — `dom_name_agrees_with_js_kind` test verifies all variants
- `SwError::default_message()` — `default_message_table` test verifies all 21 messages
- `From<StorageError>` — `from_storage_error` test covers all 7 variants
- `From<NetworkError>` — `from_network_error` test covers all 7 variants
- `From<ScriptFetchError>` — `from_script_fetch_error` test covers all 6 variants
- `From<ScriptEvalError>` — `from_script_eval_error` test covers all 3 variants
- `Display` for all error types — `display_strings_are_stable` test

### key.rs
- `StorageKey::from_origin()` — `from_origin_serializes` and `from_origin_rejects_opaque` tests
- `StorageKey::from_raw()` — `from_raw_is_verbatim` test
- `StorageKey::as_str()` — exercised in all StorageKey tests
- `is_potentially_trustworthy()` — `trustworthy_table` test with 16 cases (true and false)

## Review notes (2026-09-04)

Independently re-verified against `tasks/02_TASK_ERROR_IDS_KEY_CLOCK.md` §4/§5, with
`CARGO_TARGET_DIR=/tmp/boa-sw-target` (local disk) to avoid `vboxsf` shared-folder I/O slowness.
All 15 acceptance criteria confirmed as claimed above, with one finding, fixed on this branch:

- `error::tests::js_kind_is_total` built a duplicate `match` over `ALL` but discarded the result
  (`let _ = match err { ... }`) instead of calling `err.js_kind()` and asserting against the
  table. It only guaranteed the *test's own copy* of the match stayed exhaustive at compile time;
  it never verified the real function's output. The companion test
  `dom_name_agrees_with_js_kind` calls the real `js_kind()`, but only checks `dom_name()` against
  it — both derived from the same call, so that check is tautological. Net effect: no test
  verified `SwError::js_kind()` against the TS §12 table, so a regression in the mapping (e.g. a
  swapped `Dom` name) would have passed the whole suite silently — a real gap given `AD-12` makes
  `js_kind()` the sole source bindings trust. The production mapping itself was hand-checked
  against the table and is correct; this was a test-coverage gap, not a live bug.
  **Fixed:** the match now assigns to `expected` and the test asserts
  `err.js_kind() == expected` for all 21 variants. Re-ran criteria 2, 3, 5, 8, 9, 10 after the
  fix — all still green (21 unit + 1 integration test passing under `--all-features`, clippy
  clean, fmt clean, no wildcard arm introduced).

**Verdict: T-02 accepted.**

## Deviations

None expected. No new dependencies added.

## Decisions

No new `DECISIONS.md` entries required. All choices specified in the TS.

## Open questions for T-03

None. T-03 (URL layer) builds directly on `key.rs` and `error.rs` which are now complete.
