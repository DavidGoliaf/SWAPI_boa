//! Engine-independent core of the Service Workers implementation: model, job queue, lifecycle algorithms, fetch decision logic, cache algorithms and storage traits. Contains no dependency on a JavaScript engine (TS AD-2, R2.2.2).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-02`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
