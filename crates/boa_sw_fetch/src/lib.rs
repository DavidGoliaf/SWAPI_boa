//! The Fetch API subset Service Workers need: `Headers`, `Request`, `Response`, the Body mixin and `fetch()`, with request `mode`/`credentials`/`destination` and response `type`/`redirected` modelled (TS AD-8).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status: skeleton.** Implementation starts in `T-12`.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
