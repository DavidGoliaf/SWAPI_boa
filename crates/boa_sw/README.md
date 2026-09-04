# boa_sw

Service Workers for the Boa JavaScript engine: the runtime (realms, scheduler, host API) and every JavaScript-visible class (TS §4, §5, §11).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: the L3 integration layer; it may depend on all L1/L2 crates and backends.**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §3.3, §3.4, §4, §5, §10, §11.

Status: skeleton; implementation starts in work order `T-13`.
