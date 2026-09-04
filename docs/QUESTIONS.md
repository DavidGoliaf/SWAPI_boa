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
