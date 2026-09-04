//! Service Workers for the Boa JavaScript engine: the runtime (realms, scheduler, host API) and every JavaScript-visible class (TS §4, §5, §11).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-13`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
