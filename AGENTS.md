# AGENTS.md — Working Contract for Implementers (`boa-sw`)

This file is the entry point for **any** engineer or AI coding agent working in this repository.
It is tool-agnostic; if your harness reads a different file name (CLAUDE.md, .cursorrules, …),
that file only points here.

The normative specification is **`TZ_boa_sw_ServiceWorkers.md`** (referred to as "the TS").
Section references like "§7.4" and requirement ids like `R7.4.1` point into it.

## Ground rules

1. **One work order at a time.** Take exactly one task from TS §16, implement it fully, stop.
   Implement nothing outside it, even if it looks obvious or "while I'm here".
2. **No architectural decisions.** The TS fixes them (`AD-1 … AD-12`). If you believe one is
   wrong, or two acceptance criteria conflict, or a signature cannot compile as written —
   write an entry in `docs/QUESTIONS.md` and stop. Do not invent an alternative silently.
3. **Signatures are law.** Where the TS gives a type or function signature, match it
   character-for-character (lifetimes and `where` clauses required by the compiler excepted).
4. **Algorithms are numbered.** Where the TS gives numbered steps, the code carries a comment
   naming the step on each corresponding block (`// §7.4 step 6`).
5. **No `unsafe`.** Anywhere. `#![deny(unsafe_code)]` is on in every crate.
6. `thiserror` in libraries, `anyhow` only in binaries. No `unwrap`/`expect`/`panic!` in library
   code outside tests; documented-infallible cases need a comment naming the invariant.
7. **New dependency = ADR first.** One paragraph in `docs/DECISIONS.md` (maintained? widely used?
   permissive licence?) before the first use. Prefer the crates already listed in TS §2.3.
8. **Every behaviour change ships with tests.** CI green is part of "done".
9. Rust edition 2024, toolchain 1.91.0. `cargo fmt` and
   `cargo clippy --all-targets --all-features -- -D warnings` must pass before you declare completion.
10. Public items documented; each crate's README states its role in the pipeline.
11. No TODO/FIXME without a tracking entry (`docs/QUESTIONS.md` reference or issue link).
12. Never commit secrets, WPT corpus copies, or generated binaries.
13. **Borrow discipline (TS §3.4) is not negotiable.** Never hold a `RefCell` borrow across a call
    into JavaScript; always switch realms through `with_realm`. Most bugs in this codebase will be
    violations of these two rules.

## Workflow

- Branch per work order: `task/t-01`, `task/t-02`, … Base: `main`. No direct commits to `main`.
- Commits: imperative subject ≤ 72 chars, body explains *why*; one logical change per commit.
- Before starting, expand the task into `tasks/<NN>_TASK_<NAME>.md` using the template in
  TS Appendix F (or `tasks/TASK_TEMPLATE.md`), copying the normative text you will need.
- When all acceptance criteria of the work order pass locally, write
  `docs/reviews/T-<NN>-handoff.md`: what was built, the exact commands to reproduce the
  acceptance criteria, deviations (there should be none), `docs/DECISIONS.md` entries added,
  and open questions. Then **stop and hand off** — do not start the next work order.
- Rework after review: fix findings on the same branch, update the handoff file, resubmit.
- After finishing and before committing: do a retrospective pass over your own diff looking for
  bugs, then fix what you find.

## Definition of Done (every work order)

`cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean;
`cargo deny check` clean; `cargo test --all-features` green on Linux and one other CI platform;
every acceptance criterion of the work order demonstrably passing (commands recorded in the
handoff); `docs/traceability.md` updated with the requirement ids you covered; docs updated;
`docs/DECISIONS.md` updated for any choice made; handoff file written.

## Escalate (stop and ask) when

- Acceptance criteria conflict with each other or with the TS.
- A `boa_engine` 0.22 API makes a requirement impractical (check `docs/BOA_022_PLATFORM_NOTES.md`
  first — it may already be answered).
- You need `unsafe`, a build script with network access, or a non-permissively licensed crate.
- The estimated diff exceeds the task's budget by more than 50 % — the task is mis-scoped;
  say so instead of delivering a monster.
- The Spec text and the TS disagree. The Spec wins on behaviour, the TS wins on architecture;
  record the discrepancy in `docs/QUESTIONS.md` either way.
