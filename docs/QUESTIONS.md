# Escalations

Anything an implementer must not decide alone goes here (see `AGENTS.md`, "Escalate").
One section per question, newest last. Do not delete answered questions; mark them **Answered**.

Template:

## Q-<NN> — <short title>
- **Raised by / task:** <name> / `T-NN`
- **Blocking?** yes | no (what is blocked)
- **Context:** what you were doing, which requirement id is involved
- **Options considered:** at least two, with consequences
- **Recommendation:** yours
- **Answer:** *(filled by the architect/customer)*

## Q-01 — Spec revisions unavailable at T-01 time
- **Raised by / task:** T-01 implementer / `T-01`
- **Blocking?** no (skeleton proceeds; pinning deferred)
- **Context:** `docs/SPEC_REVISION.md` requires pinned Spec SHAs/dates; the build environment (VirtualBox shared folder, offline-first) could not retrieve them during T-01
- **Options considered:** (1) guess SHAs — rejected, forbidden by the work order; (2) record `unavailable (2026-09-04)` and pin later — chosen
- **Recommendation:** a later task with network access fills the SHAs
- **Answer:** *(filled by the architect/customer)*
