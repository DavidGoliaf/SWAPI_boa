//! Minimal `DOM` shim required by Service Workers: `EventTarget`, `Event`, `DOMException`, `MessageEvent`, `ErrorEvent` and `AbortController` for the `Boa` engine (TS §5.5).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-11`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
