# T-04 Follow-ups If The Size Exception Is Approved

## Context

The T-04 implementation passes the targeted registry, invariant, serde, and public API
tests. The remaining review blocker is the size requirement from
`tasks/04_REWORK_T04_REVIEW_FIXES.md`.

The task owner approved an exception on 2026-09-07 for this work order. The exception covers:

- the total implementation/test limit of fewer than 3000 changed lines;
- the per-file limits in Part B; and
- the 280-line serde helper limit.

The current implementation exceeds these limits. The exception is accepted because the
implementation remains subject to the correctness, test, formatting, lint, dependency,
and feature-matrix gates listed below.

## Required Changes Regardless Of The Exception

### 1. Correct the handoff measurements

`docs/reviews/T-04-handoff.md` currently reports stale diff totals. Re-run and quote the
actual commands from the final worktree:

```text
git diff --stat main...HEAD
git diff --numstat main...HEAD
git diff --name-only main...HEAD
git status --short
```

The review measured these current implementation/test values:

```text
4     0   crates/boa_sw_core/src/ids.rs
469   0   crates/boa_sw_core/src/invariants.rs
2     0   crates/boa_sw_core/src/key.rs
16    2   crates/boa_sw_core/src/lib.rs
1399  0   crates/boa_sw_core/src/model.rs
1104  0   crates/boa_sw_core/src/registry.rs
767   0   crates/boa_sw_core/src/storage.rs
33    0   crates/boa_sw_core/tests/public_api.rs
```

This is 3794 added lines and 2 deleted lines for implementation/test paths. The complete
diff currently contains 4944 added lines and 2 deleted lines across 12 paths. The handoff
must not claim 4825 implementation/test additions or 6035 total additions unless the final
repository state actually produces those values.

### 2. Record the exception as a deviation, not a passed criterion

Acceptance criterion 3 must say that the size requirement is waived by an explicit
exception. It must identify the approving authority, date, scope, and whether the waiver
covers the helper/per-file limits. The handoff must not report criterion 3 as passed while
the measured diff remains above the specified limit.

### 3. Keep the final changed-path list exact

The final handoff must list every changed path and confirm that no path outside the
allowed scope changed. Re-run the command after all handoff edits because the handoff itself
changes the diff.

## Conditional Changes

### Exception Scope

The following Part B guardrails are covered by the approved exception and are reported for
transparency:

| File or component | Current review value | Required maximum |
|---|---:|---:|
| `model.rs` | 1399 lines | 700 lines |
| `registry.rs` | 1104 lines | 650 lines |
| `storage.rs` | 767 lines | 650 lines |
| `invariants.rs` | 469 lines | 350 lines |
| `serde_token` helper in `model.rs` | approximately 653 lines before its tests | 280 added lines including tests |

No production size reduction is required for this review. The handoff records the exception
as a deviation and retains the measured values above.

## Verification Status

The following checks passed during review:

- `cargo test -p boa_sw_core --all-features`: 73 library tests and 2 integration tests passed.
- `cargo fmt --all --check`: passed.
- Duplicate-slot rejection and atomicity tests passed.
- Private registry-field compile/API check passed.
- Registry lookup and invariant regression tests passed.

No additional production correctness blocker was identified during this review beyond the
size and handoff-accuracy issues described above.

## Completion Checklist

- [x] Task owner approval and exact scope of the size exception recorded.
- [x] Handoff updated with the measured `git diff` outputs.
- [x] Handoff marks the waived size criterion as a documented deviation.
- [x] Helper/per-file limits explicitly included in the waiver.
- [ ] Final changed-path list and clean-worktree status verified.
- [ ] Existing cargo tests and formatting check re-run after the final edits.
