# Escalations

Anything an implementer must not decide alone goes here (see `AGENTS.md`, "Escalate").
One section per question, newest last. Do not delete answered questions; mark them **Answered**.

Template (fenced so it renders as a template; count real questions with `grep -c '^## Q-[0-9]'`):

```markdown
## Q-<NN> — <short title>
- **Raised by / task:** <name> / `T-NN`
- **Blocking?** yes | no (what is blocked)
- **Context:** what you were doing, which requirement id is involved
- **Options considered:** at least two, with consequences
- **Recommendation:** yours
- **Answer:** **Answered 2026-09-04 (architect).** All six revisions are now pinned in
  `docs/SPEC_REVISION.md`: Service Workers = W3C CRD of 12 August 2026
  (`https://www.w3.org/TR/2026/CRD-service-workers-20260812/`), plus WHATWG commit snapshots for
  Fetch, DOM, HTML and URL. Later tasks quote **that** text; if a spec page has moved on, read the
  commit snapshot URL, not the living page. No further action for T-01.
```

## Q-01 — Spec revisions unavailable at T-01 time
- **Raised by / task:** T-01 implementer / `T-01`
- **Blocking?** no (skeleton proceeds; pinning deferred)
- **Context:** `docs/SPEC_REVISION.md` requires pinned Spec SHAs/dates; the build environment (VirtualBox shared folder, offline-first) could not retrieve them during T-01
- **Options considered:** (1) guess SHAs — rejected, forbidden by the work order; (2) record `unavailable (2026-09-04)` and pin later — chosen
- **Recommendation:** a later task with network access fills the SHAs
- **Answer:** **Answered 2026-09-04 (architect).** All six revisions are now pinned in
  `docs/SPEC_REVISION.md`: Service Workers = W3C Candidate Recommendation Draft of 12 August 2026
  (`https://www.w3.org/TR/2026/CRD-service-workers-20260812/`), plus WHATWG commit snapshots for
  Fetch (`394d20d1`), DOM (`a2331a45`), HTML (`6a6ee7a8`) and URL (`55d66993`). Later tasks quote
  **that** text; when a living standard has moved on, read its commit-snapshot URL, not the live
  page. Status: **Answered**, no further action for T-01.

## Q-02 — `scope_matches`: fragment excluded from scope, client, or both
- **Raised by / task:** reviewer / `T-03`
- **Blocking?** no (T-03 proceeds with the work-order reading; T-04 unblocked either way)
- **Context:** work order `tasks/03_TASK_URL_LAYER.md` §3.1 specifies `scope_matches` as true iff
  the serialized `scope` (fragment excluded) is a string prefix of the serialized `client_url`
  (fragment excluded), while TS `R6.2.2` excludes the fragment from the client URL only.
  The as-implemented `scope_matches` excluded the fragment from the client side only, following
  the TS literally and violating the work order's signature contract.
- **Options considered:** (1) follow the TS literally (strip client fragment only) — diverges from
  the work order contract; (2) follow the work order (strip both sides) — on all valid inputs
  identical to (1), because stored scopes never carry a fragment (`R6.1.3`); differs only for
  direct calls with an un-normalized scope.
- **Recommendation:** option (2) — implemented in `url_util.rs::scope_matches` with a doc comment
  referencing this question; two `scope_matches_table` cases pin the behaviour
  (`scope` with `#frag` matches). No observable difference for `T-04`'s `match_registration`.
- **Answer:** **Answered 2026-09-07 (T-03 acceptance).** Option (2) is accepted: `scope_matches`
  excludes fragments from both the scope and client serialized URLs. This is the work-order
  contract, is pinned by the direct-call tests, and is behaviorally identical to the TS wording
  for normalized stored scopes because `R6.1.3` forbids stored scope fragments. Q-02 is closed;
  T-04 may use `url_util::scope_matches` without additional fragment handling.

## Q-03 — `path_restriction_ok`: malformed `Service-Worker-Allowed` value
- **Raised by / task:** reviewer / `T-03`
- **Blocking?** no (T-03 proceeds; behaviour pinned by tests)
- **Context:** `R6.2.3` and work order `tasks/03_TASK_URL_LAYER.md` §3.1 do not state what happens
  when the already-extracted `allowed` value fails to parse against `script_url`. The §4 test
  table lists a malformed-`allowed` case as `Err`, which reads as if malformed input itself
  produced the error.
- **Options considered:** (1) propagate `SwError::InvalidUrl` on malformed `allowed` — rejected:
  contradicts `AD-12` uniformity (function's error is `SwError::PathRestriction`) and surfaces a
  header-syntax problem as a URL-parse error; (2) treat malformed `allowed` as absent (override
  does not apply, fall through to the script-directory check) — chosen, matching the §3.1 wording
  ("unless `allowed` is `Some(value)` **and** `value` parses ...").
- **Recommendation:** option (2) — implemented; `path_restriction_ok_table` covers both
  fallthrough outcomes (`Err` when the default check also fails, `Ok` when it passes).
- **Answer:** **Answered 2026-09-07 (T-03 acceptance).** Option (2) is accepted: a malformed
  `Service-Worker-Allowed` value is treated as absent, so the default script-directory restriction
  is evaluated. The function returns `SwError::PathRestriction` when that restriction fails and
  `Ok(())` when it already passes; it does not propagate `SwError::InvalidUrl`. This behavior is
  pinned by both malformed-header table cases. Q-03 is closed and does not block T-04.
