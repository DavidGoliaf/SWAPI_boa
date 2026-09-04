# boa_sw_wpt

web-platform-tests harness for boa_sw: runs the service-workers test suite against a fake host and compares results with expectations.json (TS §15.3).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: the conformance tool on top; it may depend on everything but nothing may depend on it.**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §15.3.

Status: skeleton; implementation starts in work order `T-24`.
