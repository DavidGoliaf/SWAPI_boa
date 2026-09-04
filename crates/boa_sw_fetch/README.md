# boa_sw_fetch

The Fetch API subset Service Workers need: Headers, Request, Response, the Body mixin and fetch(), with request mode/credentials/destination and response type/redirected modelled (TS AD-8).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: an L2 crate; it may depend on `boa_sw_core` and `boa_engine`, never the reverse.**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §5.4, §8, AD-7, AD-8.

Status: skeleton; implementation starts in work order `T-12`.
