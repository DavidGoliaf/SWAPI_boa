//! In-memory implementation of the `boa_sw_core` storage traits. Intended for tests and for hosts that do not need persistence; the default backend is `boa_sw_sqlite` (TS D-1).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-08`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
