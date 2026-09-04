# boa_sw_dom

Minimal DOM shim required by Service Workers: EventTarget, Event, DOMException, MessageEvent, ErrorEvent and AbortController for the Boa engine (TS §5.5).

## Position in the pipeline

`boa_sw_core` (model + algorithms, no engine)
→ `boa_sw_fetch` / `boa_sw_dom` (JavaScript classes)
→ `boa_sw` (runtime + host API)
with `boa_sw_memory` / `boa_sw_sqlite` behind the storage traits and `boa_sw_wpt` on top.
**This crate is: an L2 leaf; it may depend on `boa_engine`/`boa_gc` but never on `boa_sw_core`.**

## Working on this crate

Read [`AGENTS.md`](../../AGENTS.md) first — it is the working contract for every change.
The normative specification is [`TZ_boa_sw_ServiceWorkers.md`](../../TZ_boa_sw_ServiceWorkers.md);
the sections that govern this crate are: §5.5, §3.3.

Status: skeleton; implementation starts in work order `T-11`.
