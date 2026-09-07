//! Engine-independent core of the Service Workers implementation: model, job queue, lifecycle algorithms, fetch decision logic, cache algorithms and storage traits. Contains no dependency on a JavaScript engine (TS AD-2, R2.2.2).
//!
//! Part of the `boa-sw` workspace. The normative specification is
//! `TZ_boa_sw_ServiceWorkers.md`; the working contract for implementers is `AGENTS.md`.
//!
//! **Status:** records, registry and storage traits (`T-04`): model records, registry
//! lookups, `SwStorage` interface and the invariants checker, on top of the `T-03` URL layer.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clock;
pub mod error;
pub mod ids;
pub mod invariants;
pub mod key;
pub mod model;
pub mod observe;
pub mod registry;
pub mod storage;
pub mod url_util;

pub use clock::Clock;
pub use error::{
    JsErrorKind, NetworkError, ScriptEvalError, ScriptFetchError, StorageError, StorageResult,
    SwError, SwResult,
};
pub use ids::{CacheId, ClientId, DispatchId, IdAllocator, JobId, RegistrationId, WorkerId};
pub use key::{StorageKey, is_potentially_trustworthy};
pub use model::{
    EventType, RegistrationRecord, RunState, ScriptResource, ScriptResourceMap, UpdateViaCache,
    WorkerRecord, WorkerState, WorkerType,
};
pub use observe::{NullObserver, ObserverEvent, SwObserver, WarningCode};
pub use registry::Registry;
pub use storage::{
    CacheBatchReport, CacheEntry, CacheEntryId, CacheName, CacheOperation, CacheQuery,
    CacheRequestKey, CacheResponse, CacheResponseKind, PersistedRegistration, StorageBatch,
    StorageOp, SwStorage,
};
