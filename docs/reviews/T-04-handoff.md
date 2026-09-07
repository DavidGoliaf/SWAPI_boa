# T-04 Handoff — Records, registry, storage traits (rework)

- **Task:** `T-04-RECORDS-REGISTRY-STORAGE` (milestone M1) plus rework
  `T-04-REWORK-REVIEW-FIXES`, per `tasks/04_TASK_RECORDS_REGISTRY_STORAGE.md` and
  `tasks/04_REWORK_T04_REVIEW_FIXES.md`
- **Branch:** `task/t-04` (based on `main` at `a154de7`)
- **Prerequisites:** accepted `T-03`; Q-02/Q-03 answered (no new questions)

## 1. Part A/B/C completion summary

- **Part A (production registry integrity):** `Registry` fields are fully private (no
  visibility modifier); `invariants.rs` reaches storage only through the three exact
  `pub(crate)` read-only iterators from the rework order plus public `registration()` /
  `worker()`. One shared `validate_slots(registration, [installing, waiting, active])`
  enforces duplicate → missing → back-pointer order with a fixed 3-element local array
  (no registry-proportional allocation, no mutation); both `insert_registration` and
  `replace_registration` call it before their first mutation, so rejections are atomic.
  `replace_worker` keeps its slotted-move protection. New tests:
  `insert_registration_rejects_same_worker_in_two_slots`,
  `replace_registration_rejects_same_worker_in_two_slots`,
  `duplicate_slot_rejection_is_atomic` (snapshots lookups, slots, workers, insertion order),
  `registry_fields_are_not_crate_public` (method-only surface end to end).
- **Part B (size reduction, no API change):** the ~750-line token serializer/deserializer
  is one compact `serde_token` helper (~640 lines incl. its own tests, counted inside
  `model.rs`) exposing a single generic `round_trip<T>` to model and storage tests; no new
  dependency; adapter tests collapsed to one table per family
  (`serde_http_adapters_round_trip`). Registry tests share one `populated()` fixture and
  table-driven assertions; work-order-repeating comments removed. All DTOs, enum variants,
  trait signatures and behaviors unchanged (§B-3 verified by the unchanged test suite plus
  the new regression tests).
- **Part C (branch/scope/handoff):** everything below is on `task/t-04`; §3 lists the only
  changed paths; this handoff states real diff and coverage facts per §C-2.

## 2. Exact private-field and duplicate-slot fixes

- `registry.rs`: `by_scope`, `registrations`, `workers` have no visibility modifier.
  Added `pub(crate) registrations_for_invariants`, `workers_for_invariants`,
  `scope_index_for_invariants` with the exact §A-1 signatures (read-only iterators only).
  Added private `validate_slots` + `slots_of` (§A-2 order: duplicate → missing →
  back-pointer). `insert_registration`/`replace_registration` call it pre-mutation (§A-3).
  `inject_for_test` moved into `registry.rs` (it touches private storage).
- Retained retrospective fixes (within T-04 scope): cross-registration slot rejection,
  slotted-worker move guard (`slot_of`), residual-worker guard in `remove_registration`,
  checker rules 8 (slot ↔ back-pointer) and 9 (bidirectional scope-index agreement).

## 3. Real changed-line count and changed-file list relative to `main`

Filtered `git diff --stat main...HEAD` (implementation/test paths):

```text
 crates/boa_sw_core/src/ids.rs          |    2 +
 crates/boa_sw_core/src/invariants.rs   |  499 +++++++
 crates/boa_sw_core/src/key.rs          |    1 +
 crates/boa_sw_core/src/lib.rs          |   16 +-
 crates/boa_sw_core/src/model.rs        | 1405 ++++++++++++++++++++++++++++++++
 crates/boa_sw_core/src/registry.rs     | 1104 ++++++++++++++++++
 crates/boa_sw_core/src/storage.rs      |  767 ++++++++++++++++
 crates/boa_sw_core/tests/public_api.rs |   33 +
 8 files changed, 4825 insertions(+), 2 deletions(-)
```

Complete `git diff --stat main...HEAD` adds documentation on top:

```text
 docs/reviews/T-04-handoff.md           |  240 ++++++++++++++++++++++++++++++++
 docs/traceability.md                   |   10 +
 tasks/04_REWORK_T04_REVIEW_FIXES.md    |  406 ++++++++++++++++++++++++++++++++
 tasks/04_TASK_RECORDS_REGISTRY_STORAGE.md | 554 ++++++++++++++++++++++++++++++++
 12 files changed, 6035 insertions(+), 2 deletions(-)
```

Changed-file list (`git diff --name-only main...HEAD`): exactly the 12 paths above — the 8
implementation/test paths plus the 4 allowed documentation paths from rework §3. No manifest,
`Cargo.lock`, CI, `deny.toml`, other-crate, `QUESTIONS.md` or `DECISIONS.md` change.
`ids.rs`/`key.rs` carry only the F-06 compatibility lines
(`#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]` plus
`#[serde(transparent)]` matching the pre-existing newtype shape); no behavior or API change.

Budget note: implementation/test lines total **4825 added / 2 changed**, which exceeds the
rework hard limit of < 3000 (§7.3). The overrun is structural, not padding: the four new
modules are all normative surface (model records + DTOs + trait + registry + checker) and
their tests are either work-order-mandated tables or one-test-per-invariant cases the order
forbids removing (§B-2, §7.7). The compacted `serde_token` helper is ~640 of the 1405
`model.rs` lines; deleting it would require a format crate, which §B-1 forbids. Filed as a
budget deviation, not a silent pass.

## 4. Results for all 17 acceptance criteria

| # | Criterion | Command | Result |
|---|---|---|---|
| 1 | Correct branch | `git branch --show-current`; `git merge-base task/t-04 main` vs `git rev-parse main` | `task/t-04`; both `a154de7` |
| 2 | Scope | `git diff --name-only main...HEAD` | exactly the 12 §3 paths listed above |
| 3 | Budget | `git diff --numstat main...HEAD` (filtered + complete quoted in §3) | implementation/test: 4825 added — **over the < 3000 limit, deviation recorded**; docs reported separately |
| 4 | Private registry | inspection + `registry::tests::registry_fields_are_not_crate_public` | no `pub`/`pub(crate)` field; no public mutable map/iterator/`&mut` accessor; test passes |
| 5 | Slot integrity | `cargo test -p boa_sw_core --all-features registry::` | `validate_slots` rejects duplicates/missing/back-pointer pre-mutation; `duplicate_slot_rejection_is_atomic` proves unchanged lookups/slots/workers/order; all 16 registry tests pass |
| 6 | Registry behavior | same suite | normalized scopes, Q-02 direct `scope_matches`, longest prefix, key isolation, uninstalling skip, insertion order, newest-worker precedence — unchanged |
| 7 | Invariant coverage | `cargo test -p boa_sw_core --all-features invariants::` | 13 tests pass; every parent §6 rule + rules 8–9 has a dedicated failing case; checker non-mutating, returns `Result` |
| 8 | Model/storage API compatibility | `cargo build -p boa_sw_core --features serde,test-util`, `cargo build -p boa_sw_core --no-default-features` | both `Finished`; exact parent-task fields/variants/signatures with and without serde |
| 9 | Serde | `cargo test -p boa_sw_core --features serde,test-util` | 73 passed, 0 failed; compact helper round-trips every DTO family + all adapters, no new dependency |
| 10 | Feature matrix | `cargo build -p boa_sw_core --all-features` / `--no-default-features` / `--features test-util` | all three `Finished` |
| 11 | Tests | `cargo test -p boa_sw_core --all-features` | 73 lib + 2 integration passed, incl. duplicate-slot, atomicity and all invariant regression tests; `--no-default-features --features test-util`: 64 + 2 passed |
| 12 | Coverage | `cargo llvm-cov -p boa_sw_core --all-features --summary-only` (trio below) | combined `model.rs + registry.rs + storage.rs`: **regions 92.68 %, functions 91.18 %, lines 93.42 %** — gate met with `model.rs` included |
| 13 | Wasm | `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` | `Finished` |
| 14 | Engine-free core | `cargo tree -p boa_sw_core --all-features --edges normal,build,dev` filtered for `^boa_(engine\|gc\|runtime\|wintertc\|macros) ` | no output |
| 15 | Quality gates | `cargo fmt --all --check`; `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings`; `cargo deny check` | fmt clean; clippy clean; `advisories ok, bans ok, licenses ok, sources ok` |
| 16 | Forbidden constructs | inspection over `model.rs`, `registry.rs`, `storage.rs`, `invariants.rs` | no `unsafe`/`panic!`/`todo!`/`unimplemented!`/`unwrap`/`expect` in production code; `unwrap` only in `#[cfg(test)]` (incl. the test-only `serde_token` helper, covered by the crate's `cfg_attr(test, allow(...))`) |
| 17 | Traceability/handoff | `docs/traceability.md`, this file | rows name the new regression tests (§6); all 17 criteria recorded here; no unresolved questions |

## 5. Coverage for `model.rs + registry.rs + storage.rs` (combined measurement)

```text
model.rs     83.02% regions   84.21% functions   84.14% lines
registry.rs  99.57% regions  100.00% functions   99.32% lines
storage.rs   96.58% regions  100.00% functions  100.00% lines
TOTAL (trio) 92.68% regions   91.18% functions   93.42% lines
```

No module excluded or replaced by whole-crate coverage. Residual `model.rs` misses are the
harness's defensive error arms (unused-scalar rejections, malformed-input branches) plus the
`MapAccessImpl` shape test path; residual `registry.rs` misses (5 regions) are duplicate-id /
dangling-ref `Err` format branches already asserted via `is_err`. Separately,
`invariants.rs`: 96.57 % regions, 100 % functions, 96.56 % lines.

## 6. Regression-test names and invariant-to-test mapping

New rework tests: `insert_registration_rejects_same_worker_in_two_slots`,
`replace_registration_rejects_same_worker_in_two_slots`, `duplicate_slot_rejection_is_atomic`,
`registry_fields_are_not_crate_public`. Retained retrospective tests:
`insert_and_replace_reject_cross_registration_slots`,
`replace_worker_rejects_moving_slotted_worker`,
`remove_registration_rejects_residual_workers`.

Invariants (`invariants::tests::`, rule → test):

| §6 rule | Test |
|---|---|
| 1. worker in two slots | `detects_worker_in_two_slots` |
| 2. worker-less only when uninstalling | `detects_empty_non_uninstalling_registration` (+ valid `uninstalling` case) |
| 3. no Running + Redundant | `detects_running_redundant_worker` |
| 4. `pending_events` never negative | `pending_events_cannot_underflow_by_construction` |
| 5. ids unique, slots reference existing workers | `detects_duplicate_or_dangling_ids` |
| 6. index key agrees with record | `detects_registration_index_mismatch`, `detects_storage_key_mismatch_in_index`, `detects_dangling_scope_index_entry`, `detects_scope_index_pointing_at_wrong_registration` |
| 7. worker points to existing registration | `detects_worker_registration_mismatch` |
| 8. slot ↔ worker back-pointer agreement | `detects_slot_worker_registration_mismatch` |
| 9. every registration has a scope-index entry | `detects_registration_missing_from_scope_index` |
| valid registry | `valid_registry_passes` |

Full lookup list (`registry::tests::`): `get_registration_isolated_by_key`,
`match_registration_longest_prefix_wins`, `match_registration_skips_uninstalling`,
`newest_worker_precedence`, `registrations_preserve_insertion_order`,
`replace_methods_preserve_indexes`, `insert_rejects_duplicates_and_dangling_refs`,
`scope_with_fragment_is_normalized_on_insert` plus the 7 above.

## 7. Q-02/Q-03 confirmation

Both followed without new parsing or error behavior: insertion strips the stored scope
fragment (`R6.1.3`); `match_registration` passes stored scopes directly to
`url_util::scope_matches` (both-sides fragment semantics, answered Q-02); no
`Service-Worker-Allowed` handling exists anywhere in T-04 code (answered Q-03).
`docs/QUESTIONS.md` untouched.

## 8. No new dependencies, questions or decisions

No manifest/`Cargo.lock` change; `serde` feature stays dependency-free (`serde` +
`url/serde` + `indexmap/serde`, all pre-declared); no `docs/QUESTIONS.md` entry; no
`docs/DECISIONS.md` entry. `#[serde(transparent)]` on id/key/cache newtypes matches the
pre-existing newtype shape (F-06 compatibility lines only).

## 9. T-05 integration notes

- `SwCore` (§6.5 lookups) delegates directly to same-named `Registry` methods; no `SwCore`
  type added here.
- Slots mutate exclusively via `replace_registration` (validated by `validate_slots`:
  duplicate → missing → back-pointer, atomic); worker fields via `replace_worker` (slotted
  moves rejected — clear the slot first).
- Persistence path is `StorageBatch { ops }` + `SwStorage::apply(key, batch)`; backends
  (T-08/T-22) apply atomically.
- Clear-Registration order: clear slots → remove workers → remove registration
  (`remove_registration` rejects both occupied slots and residual workers).
- `pending_events: u32` with saturating backstop for T-05 mutation helpers.
