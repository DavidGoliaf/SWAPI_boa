//! Engine-independent core of the Service Workers implementation: model, job queue, lifecycle algorithms, fetch decision logic, cache algorithms and storage traits. Contains no dependency on a JavaScript engine (TS AD-2, R2.2.2).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status:** URL layer (`T-03`): parsing, scope matching, path restriction on top of the
//! `T-02` foundations (errors, ids, storage key, clock, observer).

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clock;
pub mod error;
pub mod ids;
pub mod key;
pub mod observe;
pub mod url_util;

pub use clock::Clock;
pub use error::{
    JsErrorKind, NetworkError, ScriptEvalError, ScriptFetchError, StorageError, StorageResult,
    SwError, SwResult,
};
pub use ids::{CacheId, ClientId, DispatchId, IdAllocator, JobId, RegistrationId, WorkerId};
pub use key::{StorageKey, is_potentially_trustworthy};
pub use observe::{NullObserver, ObserverEvent, SwObserver, WarningCode};
