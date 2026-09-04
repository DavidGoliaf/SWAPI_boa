# boa_sw_core

Engine-independent core of the Service Workers implementation: model, job queue, lifecycle algorithms, fetch decision logic, cache algorithms and storage traits. Contains no dependency on a JavaScript engine (TS AD-2, R2.2.2).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: the bottom layer (L1); it may depend only on the shared crates of TS §2.3 and never on `boa_engine` or siblings.**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §6, §7, §8, §9, §12, §13.

Status: skeleton; implementation starts in work order `T-02`.
