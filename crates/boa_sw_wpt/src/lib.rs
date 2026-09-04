//! `web-platform-tests` harness for `boa_sw`: runs the `service-workers` test suite against a fake host and compares results with `expectations.json` (TS §15.3).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-24`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
