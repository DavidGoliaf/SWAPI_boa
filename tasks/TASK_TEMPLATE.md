# WORK ORDER: T-<NN> — <title>

| Metadata | Value |
|---|---|
| **Task id** | `T-<NN>-<SHORT-NAME>` |
| **Milestone** | M<n> |
| **Target crates** | `crates/...` |
| **Normative TS** | `TZ_boa_sw_ServiceWorkers.md` §…, §… |
| **Prerequisites** | accepted `T-<NN>`, `T-<NN>` |
| **Diff budget** | ~N lines (stop and escalate above +50 %) |
| **Architect's role** | Signatures, algorithms and file layout are already decided below. |
| **Implementer's role** | Mechanical implementation of exactly what is written. **No architectural decisions.** |

---

## 1. Guardrails

Copy verbatim from the TS the decisions that constrain this task, at minimum:

- the relevant `AD-*` entries (TS §3.2);
- the borrow-discipline rules `R3.4.1`–`R3.4.6` for anything touching `SwRuntime` or a `Context`;
- the GC/object-model rules `R3.3.1`–`R3.3.5` for anything creating JS objects;
- the error-mapping table rows (TS §12) the task can produce.

## 2. Files to create / modify

```
crates/<crate>/src/<file>.rs      # one line describing the purpose
crates/<crate>/tests/<file>.rs
```

Nothing outside this list may be touched, except `docs/traceability.md`,
`docs/DECISIONS.md`, `docs/QUESTIONS.md` and the handoff file.

## 3. Signatures (normative — copy, do not redesign)

```rust
// every public type, trait and function this task must produce
```

## 4. Algorithms (numbered steps)

Copy the numbered steps from the TS section, keeping the numbering, and annotate each with the
requirement id it satisfies. The implementation must carry `// §X.Y step N` comments.

## 5. Tests to write

| Test name | Asserts |
|---|---|
| `mod::test_...` | … |

State for each test: the fixture (fake clock/http/storage), the exact inputs, and the exact
expected output (including snapshot names for `insta`).

## 6. Acceptance criteria

Numbered and verifiable, each with the command that proves it:

1. `cargo test -p <crate> --all-features` — all listed tests pass.
2. `cargo clippy --all-targets --all-features -- -D warnings` — clean.
3. `cargo fmt --check` — clean.
4. `cargo deny check` — clean.
5. …task-specific, measurable criteria…

## 7. Do not do

- Explicit out-of-scope list (features belonging to later tasks, refactors, renames).

## 8. Handoff

Write `docs/reviews/T-<NN>-handoff.md` containing:

1. What was built (file-by-file summary).
2. The exact commands that prove every acceptance criterion, with their output summary.
3. Deviations from this work order — there should be none; anything present must also be in
   `docs/QUESTIONS.md`.
4. `docs/DECISIONS.md` entries added.
5. Requirement ids covered, mirrored into `docs/traceability.md`.
6. Open questions for the next task.

Then stop. Do not start the next work order.
