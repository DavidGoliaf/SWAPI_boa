# boa_sw_sqlite

SQLite implementation of the boa_sw_core storage traits — the default, persistent backend for registrations, script resource maps and cache storage (TS D-1, Appendix C).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: an L0 backend; it may depend only on `boa_sw_core` (plus `rusqlite`).**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §4.2.4, §13.2, §13.6, Appendix C.

Status: skeleton; implementation starts in work order `T-22`.
