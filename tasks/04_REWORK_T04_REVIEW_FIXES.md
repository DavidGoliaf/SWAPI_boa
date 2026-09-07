# T-04 Rework — review fixes and scope reduction

| Metadata | Value |
|---|---|
| **Task id** | `T-04-REWORK-REVIEW-FIXES` |
| **Parent task** | `T-04-RECORDS-REGISTRY-STORAGE` |
| **Target branch** | `task/t-04`, based on `main` |
| **Target crate** | `crates/boa_sw_core` only |
| **Prerequisite** | T-03 accepted; T-04 implementation reviewed and returned for rework |
| **Normative sources** | `tasks/04_TASK_RECORDS_REGISTRY_STORAGE.md`, TS §4.2.4, §6.3–§6.5, `R6.3.1`, `R6.3.2`, `R15.5.3` |
| **Hard limit** | Fewer than 3000 changed implementation/test lines relative to `main`; documentation is counted separately and must remain within the allowed path list |
| **Implementation rule** | Follow this document mechanically. Do not redesign the model, storage DTOs or registry API. |

---

## 1. Review Findings Being Corrected

This rework addresses every finding from the T-04 review. The implementer must not close only the
tests while leaving the production defects or inaccurate handoff text.

### F-01 — Registry fields are crate-public

Current defect: `Registry` stores `by_scope`, `registrations` and `workers` as `pub(crate)` fields
in `crates/boa_sw_core/src/registry.rs`. Any module in the crate can mutate them directly and
bypass validation. This violates T-04 §1.9 and §4.1: registry mutation must go through methods.

Required result: all three fields are private with no visibility modifier. No production module
other than `registry.rs` may access them directly.

### F-02 — Slot uniqueness is not enforced on writes

Current defect: `insert_registration` and `replace_registration` validate that each slot points to
an existing worker belonging to the registration, but accept the same `WorkerId` in two or three
slots. The public mutation API can therefore create a registry that immediately fails invariant
rule 1.

Required result: both methods reject duplicate worker IDs among `installing`, `waiting` and
`active` before changing any state. Rejection must be atomic: the registry is byte-for-byte
equivalent in all observable fields and indexes after the failed call.

### F-03 — T-04 exceeds the line budget

The reviewed diff from the T-04 work-order base contains approximately 4060 insertions, and the
four new modules contain approximately 3818 lines including tests. The parent work order allows
fewer than 3000 changed lines and targets approximately 900 lines of implementation.

Required result: the final implementation/test diff against `main` is strictly below 3000
added/changed lines. Documentation lines are not part of the code budget, but their paths are still
subject to §3. The exact commands and outputs must be recorded in the handoff. Do not count an
untracked file as absent from the diff.

### F-04 — Work is on the wrong branch

The reviewed implementation is on `t04-spike-serde-token`; the required task branch is
`task/t-04` based on `main`. The final rework must be applied on `task/t-04`. Do not leave the
implementation only on the spike branch.

### F-05 — Handoff contradicts the actual diff and coverage rule

The current handoff says the four new modules total approximately 2900 lines and says `model.rs`
is excluded from the coverage gate, while the work order explicitly requires the combined coverage
of `model.rs`, `registry.rs` and `storage.rs`.

Required result: the handoff reports the real `git diff --stat main...HEAD`/working-tree result,
counts all changed task files, and reports the measured combined coverage for exactly
`model.rs + registry.rs + storage.rs`. No module may be silently excluded.

### F-06 — Unlisted source files are changed without an explicit scope record

The reviewed diff modifies `ids.rs` and `key.rs` to add serde derives, although they are not in
the T-04 file list. These changes are allowed only if they are required by the finalized DTO
serde contract and are explicitly listed as permitted compatibility edits below. No behavior or
API in those files may change.

---

## 2. Fixed Architecture And Non-Choices

The following decisions are final. The implementer must not choose alternatives or add questions
for them.

1. `Registry` remains the same public type and keeps the exact T-04 method surface:
   `new`, `insert_registration`, `insert_worker`, `replace_registration`, `replace_worker`,
   `remove_registration`, `remove_worker`, `registration`, `worker`, `get_registration`,
   `match_registration`, `newest_worker`, and `registrations_for_origin`.
2. The three registry indexes remain:
   `IndexMap<(StorageKey, String), RegistrationId>`,
   `HashMap<RegistrationId, RegistrationRecord>`, and
   `HashMap<WorkerId, WorkerRecord>`. Their field names remain `by_scope`, `registrations` and
   `workers`; only visibility changes to private.
3. `Registry` remains the sole owner of registry mutation. No `&mut` accessor, mutable iterator,
   `pub(crate)` field, interior-mutability wrapper or alternate mutation path may be introduced.
4. Q-02 remains closed as answered: `scope_matches` strips fragments from both serialized URLs.
   Registry insertion removes the stored scope fragment; `match_registration` passes the stored
   URL directly to `url_util::scope_matches` and does not reimplement matching.
5. Q-03 remains closed as answered: malformed `Service-Worker-Allowed` is handled by the existing
   T-03 helper as an absent override. T-04 adds no parser, no header behavior and no error mapping.
6. The exact model, storage DTO and `SwStorage` signatures from the parent work order remain
   unchanged. Do not remove a field, rename a variant, alter a trait signature or replace a DTO.
7. `serde` remains feature-gated and no new dependency is permitted. Existing local adapters may
   be retained, but test-only serialization machinery must be compacted as specified in §5.

---

## 3. Files And Allowed Changes

The final branch may change only these paths relative to `main`:

```text
crates/boa_sw_core/src/model.rs
crates/boa_sw_core/src/registry.rs
crates/boa_sw_core/src/storage.rs
crates/boa_sw_core/src/invariants.rs
crates/boa_sw_core/src/lib.rs
crates/boa_sw_core/src/ids.rs       # serde derives only; compatibility exception F-06
crates/boa_sw_core/src/key.rs       # serde derives only; compatibility exception F-06
crates/boa_sw_core/tests/public_api.rs
docs/traceability.md
docs/reviews/T-04-handoff.md
docs/reviews/T-04-exception-followups.md
tasks/04_TASK_RECORDS_REGISTRY_STORAGE.md
tasks/04_REWORK_T04_REVIEW_FIXES.md
```

No other path may change. In particular, do not change manifests, `Cargo.lock`, CI, `deny.toml`,
other crates, `docs/QUESTIONS.md` or `docs/DECISIONS.md`. The exception-followups file is a
review-only document recording the approved size exception. Q-02 and Q-03 are already answered.

The `ids.rs` and `key.rs` exception is limited to:

```rust
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
```

No other line or behavior in either file may change.

---

## 4. Part A — Production Registry Integrity

Part A is mandatory and must be completed before any size reduction or documentation update is
claimed complete.

### A-1. Make registry storage private

In `registry.rs`, change the three fields to private:

```rust
by_scope: IndexMap<(StorageKey, String), RegistrationId>,
registrations: HashMap<RegistrationId, RegistrationRecord>,
workers: HashMap<WorkerId, WorkerRecord>,
```

Keep the existing field names and types. The invariant checker may not access the fields directly
after this change. Add only private/read-only methods in `registry.rs` needed by
`invariants.rs`, with these exact signatures:

```rust
pub(crate) fn registrations_for_invariants(
    &self,
) -> impl Iterator<Item = &RegistrationRecord>;

pub(crate) fn workers_for_invariants(
    &self,
) -> impl Iterator<Item = &WorkerRecord>;

pub(crate) fn scope_index_for_invariants(
    &self,
) -> impl Iterator<Item = (&(StorageKey, String), &RegistrationId)>;
```

These methods are read-only. They must not return `&mut`, owned mutable maps, or handles that can
mutate records. `invariants.rs` must use only these iterators plus the public read-only
`registration()` and `worker()` methods.

### A-2. Add one shared slot validator

Add one private helper in `registry.rs` with this exact behavior:

```rust
fn validate_slots(
    &self,
    registration: RegistrationId,
    slots: [Option<WorkerId>; 3],
) -> Result<(), String>;
```

The helper must execute checks in this order:

1. Iterate the three slots in order `installing`, `waiting`, `active`.
2. Ignore `None`.
3. Reject a worker ID already seen in an earlier slot with an error naming both slots or the
   registration and worker ID.
4. Reject a missing worker record.
5. Reject a worker whose `registration` back-pointer differs from `registration`.
6. Return `Ok(())` only when all three checks pass.

The helper must not allocate a collection proportional to the registry. A fixed three-slot check
or a small fixed local array is sufficient. It must not mutate the registry.

### A-3. Use the helper atomically

`insert_registration` and `replace_registration` must call `validate_slots` before their first
mutation. A failed duplicate-slot, missing-worker or cross-registration validation must leave:

- the `by_scope` index unchanged;
- the registration map unchanged;
- the worker map unchanged;
- the insertion order unchanged.

Do not duplicate slot-validation logic in the two methods. `replace_worker` must keep its existing
slotted-worker move protection.

### A-4. Add source-level integrity tests

Add or retain these tests in `registry.rs`:

- `insert_registration_rejects_same_worker_in_two_slots`;
- `replace_registration_rejects_same_worker_in_two_slots`;
- `duplicate_slot_rejection_is_atomic`;
- `registry_fields_are_not_crate_public` as a compile-level/API check where practical.

The first two tests must construct a valid worker and assert that using it in `installing` plus
`waiting` returns `Err`. The atomicity test must snapshot the registration/index lookup before the
failed replacement and assert all observable values remain unchanged afterward.

---

## 5. Part B — Reduce Implementation Size Without Changing API

Part B is a code-size refactor only. It must not alter the public DTOs, registry method surface,
trait signatures or behavior fixed in Part A.

### B-1. Remove the oversized duplicated serde token harness

The current `model.rs` contains a large test-only token serializer/deserializer. Replace it with
one compact shared test helper in `storage.rs` or `model.rs`, no more than **280 added lines
including tests**, with these exact capabilities:

- serialize a value into an owned token/byte representation using only the existing `serde`
  dependency;
- deserialize the representation back into the requested type;
- support the primitives, structs, sequences, options, unit enums and newtype/struct variants
  used by T-04 DTOs;
- expose one generic `round_trip<T>` helper to both model and storage tests;
- return `Result` from helper internals; `unwrap` is permitted only in `#[cfg(test)]` callers;
- reject malformed token input rather than panic.

Do not add `serde_json`, `bincode`, `postcard`, `serde_test` or any other dependency. Do not copy
the current token harness into another module. Keep adapter-specific tests to one compact table.

### B-2. Remove redundant test-only scaffolding

Keep all behavior-required tests, but compact them as follows:

- use one shared `registration` fixture and one shared `worker` fixture;
- use table-driven cases for duplicate/missing/cross-registration errors;
- remove comments that repeat the work order without explaining a non-obvious invariant;
- remove tests that only restate `TypeId` reachability when the same type is already referenced by
  a compile-time signature test;
- keep one serde round-trip per record/DTO family and one adapter table per adapter family;
- do not remove any dedicated invariant failing-case test listed in the parent work order.

The implementation must keep these approximate upper bounds after compaction:

| File | Maximum total lines |
|---|---:|
| `model.rs` | 700 |
| `registry.rs` | 650 |
| `storage.rs` | 650 |
| `invariants.rs` | 350 |
| T-04 integration test additions | 100 |
| T-04 handoff + traceability additions | 250 |
| **Total implementation/test lines relative to `main`** | **< 3000** |

The file limits are guardrails, not permission to add filler. The hard acceptance check is the
actual implementation/test diff count relative to `main`; documentation is reported separately.

### B-3. Preserve production behavior

After compaction, production code must still provide:

- all exact model fields and enum variants;
- all exact storage DTO fields and operation variants;
- every `SwStorage` method with its exact signature;
- longest-prefix scope matching and storage-key isolation;
- uninstalling skip and newest-worker precedence;
- atomic insert/replace/remove validation;
- all invariant checks, including slot/back-pointer and bidirectional scope-index checks;
- feature-gated serde derives and adapters.

No behavior may be moved into tests. No test helper may be exposed as production API.

---

## 6. Part C — Branch, Handoff And Scope Correction

### C-1. Required branch state

The final implementation must be on `task/t-04` based on `main`. The implementer must not rewrite
or force-push history. If the current spike branch contains the implementation, transfer the
commits with a normal cherry-pick or recreate the changes on `task/t-04`; do not leave the final
result only on `t04-spike-serde-token`.

Before handoff, these commands must show the expected branch and no unrelated files:

```text
git branch --show-current
git diff --name-only main...HEAD
git status --short
```

### C-2. Correct handoff facts

Rewrite `docs/reviews/T-04-handoff.md` so it states:

- branch `task/t-04`;
- exact `git diff --stat main...HEAD` result;
- exact changed-file list;
- total changed-line count below 3000;
- no changes outside §3;
- no deviations from this rework order;
- Q-02 and Q-03 already answered, with no new question;
- measured combined coverage for exactly `model.rs`, `registry.rs` and `storage.rs`;
- separate coverage for `invariants.rs` may be reported but does not replace the required trio;
- the duplicate-slot regression tests and private-field enforcement are listed as completed.

Do not say that files are “untracked” as evidence that they are outside the diff. Do not claim
`model.rs` is excluded from the coverage criterion.

### C-3. Traceability

Keep the existing T-04 traceability rows and add/adjust only concrete test names. At minimum the
registry rows must mention:

- `R6.1.3`: fragment normalization on insert;
- `R6.3.2`: replacement methods plus private registry storage;
- `R6.5.1`: newest-worker precedence;
- `R6.5.2`: uninstalling skip;
- `R15.5.3`: every invariant failure case;
- the new duplicate-slot regression tests.

Do not edit `docs/QUESTIONS.md`: Q-02 and Q-03 are closed.

---

## 7. Acceptance Criteria

All commands run from the workspace root. The handoff must record exact output summaries.

1. **Correct branch:** `git branch --show-current` prints `task/t-04`, and `git merge-base
   task/t-04 main` equals the current `main` base used for the task.
2. **Scope:** `git diff --name-only main...HEAD` contains only the paths in §3.
3. **Budget:** the implementation/test paths in `git diff --numstat main...HEAD` total fewer than
   3000 added/changed lines; documentation totals are reported separately. Both the filtered and
   complete `git diff --stat main...HEAD` outputs are quoted in the handoff.
4. **Private registry:** no registry index field has `pub` or `pub(crate)` visibility; no public
   mutable map, iterator or `&mut` record accessor exists.
5. **Slot integrity:** `validate_slots` rejects duplicate IDs, missing workers and wrong
   back-pointers before mutation; failed calls leave all indexes and records unchanged.
6. **Registry behavior:** exact T-04 lookup behavior remains unchanged: normalized scopes,
   Q-02 direct `scope_matches`, longest prefix, storage-key isolation, uninstalling skip,
   insertion ordering and newest-worker precedence.
7. **Invariant coverage:** every invariant in parent T-04 §6 has a dedicated failing-case test;
   the checker remains non-mutating and panic-free.
8. **Model/storage API compatibility:** all exact parent-task fields, enum variants, DTO variants,
   and `SwStorage` signatures compile unchanged with and without serde.
9. **Serde:** `cargo test -p boa_sw_core --features serde,test-util` passes; the compact helper
   round-trips every model/storage DTO family and all adapters without a new dependency.
10. **Feature matrix:** these commands pass:
    `cargo build -p boa_sw_core --all-features`,
    `cargo build -p boa_sw_core --no-default-features`,
    `cargo build -p boa_sw_core --features test-util`.
11. **Tests:** `cargo test -p boa_sw_core --all-features` passes, including the duplicate-slot,
    atomicity and all invariant regression tests.
12. **Coverage:** `cargo llvm-cov -p boa_sw_core --all-features --summary-only` reports at
    least 92% combined coverage for `model.rs`, `registry.rs` and `storage.rs`; `model.rs` is not
    excluded or replaced by whole-crate coverage.
13. **Wasm:** `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features`
    passes.
14. **Engine-free core:**
    `cargo tree -p boa_sw_core --all-features --edges normal,build,dev | grep -E 'boa_(engine|gc|runtime|wintertc|macros)'`
    produces no output.
15. **Quality gates:** `cargo fmt --all --check`,
    `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` and
    `cargo deny check` pass.
16. **Forbidden constructs:** source inspection finds no `unsafe`, `panic!`, `todo!`,
    `unimplemented!`, `unwrap` or `expect` in production code added by the rework.
17. **Traceability/handoff:** traceability names the new regression tests; handoff records all
    17 criteria, actual diff facts, coverage facts, branch, and no unresolved questions.

---

## 8. Required Handoff

Update `docs/reviews/T-04-handoff.md` and stop. The handoff must include:

1. Part A/B/C completion summary.
2. The exact private-field and duplicate-slot fixes.
3. The real changed-line count and changed-file list relative to `main`.
4. Results for all 17 acceptance criteria.
5. Coverage for `model.rs + registry.rs + storage.rs` as a combined measurement.
6. The regression-test names and invariant-to-test mapping.
7. Confirmation that Q-02/Q-03 were followed without new parsing or error behavior.
8. Confirmation that no new dependencies, questions or decisions were added.
9. T-05 integration notes limited to the finalized registry methods and `SwStorage::apply`.

Then stop. Do not implement T-05 or any backend as part of this rework.
