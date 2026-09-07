//! Compile-level check of the re-exported surface.
//!
//! This test verifies that every re-exported name from `boa_sw_core` is reachable.
//! A missing or renamed export will fail to compile.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn reexports_are_reachable() {
    // Clock trait
    let _ = std::any::TypeId::of::<dyn boa_sw_core::Clock>();

    // Error types
    let _ = std::any::TypeId::of::<boa_sw_core::SwError>();
    let _ = std::any::TypeId::of::<boa_sw_core::JsErrorKind>();
    let _ = std::any::TypeId::of::<boa_sw_core::StorageError>();
    let _ = std::any::TypeId::of::<boa_sw_core::StorageResult<()>>();
    let _ = std::any::TypeId::of::<boa_sw_core::NetworkError>();
    let _ = std::any::TypeId::of::<boa_sw_core::ScriptFetchError>();
    let _ = std::any::TypeId::of::<boa_sw_core::ScriptEvalError>();
    let _ = std::any::TypeId::of::<boa_sw_core::SwResult<()>>();

    // Id types
    let _ = std::any::TypeId::of::<boa_sw_core::RegistrationId>();
    let _ = std::any::TypeId::of::<boa_sw_core::WorkerId>();
    let _ = std::any::TypeId::of::<boa_sw_core::JobId>();
    let _ = std::any::TypeId::of::<boa_sw_core::DispatchId>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheId>();
    let _ = std::any::TypeId::of::<boa_sw_core::ClientId>();
    let _ = std::any::TypeId::of::<boa_sw_core::IdAllocator>();

    // Key types
    let _ = std::any::TypeId::of::<boa_sw_core::StorageKey>();
    let _ = boa_sw_core::is_potentially_trustworthy;

    // Observer types
    let _ = std::any::TypeId::of::<boa_sw_core::NullObserver>();
    let _ = std::any::TypeId::of::<boa_sw_core::ObserverEvent<'_>>();
    let _ = std::any::TypeId::of::<boa_sw_core::WarningCode>();
    let _ = std::any::TypeId::of::<dyn boa_sw_core::SwObserver>();
}

#[test]
fn t04_exports_are_reachable() {
    // Model types (T-04 §3).
    let _ = std::any::TypeId::of::<boa_sw_core::WorkerRecord>();
    let _ = std::any::TypeId::of::<boa_sw_core::RegistrationRecord>();
    let _ = std::any::TypeId::of::<boa_sw_core::ScriptResource>();
    let _ = std::any::TypeId::of::<boa_sw_core::ScriptResourceMap>();
    let _ = std::any::TypeId::of::<boa_sw_core::WorkerState>();
    let _ = std::any::TypeId::of::<boa_sw_core::WorkerType>();
    let _ = std::any::TypeId::of::<boa_sw_core::UpdateViaCache>();
    let _ = std::any::TypeId::of::<boa_sw_core::RunState>();
    let _ = std::any::TypeId::of::<boa_sw_core::EventType>();

    // Registry (T-04 §4): reachable through methods only; no mutable fields are public.
    // `Registry` itself is `Clone + Debug + Default`; no `&mut`-returning accessor exists.
    let _ = std::any::TypeId::of::<boa_sw_core::Registry>();

    // Storage trait and DTOs (T-04 §5).
    let _ = std::any::TypeId::of::<dyn boa_sw_core::SwStorage>();
    let _ = std::any::TypeId::of::<boa_sw_core::PersistedRegistration>();
    let _ = std::any::TypeId::of::<boa_sw_core::StorageBatch>();
    let _ = std::any::TypeId::of::<boa_sw_core::StorageOp>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheName>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheEntryId>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheRequestKey>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheResponse>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheResponseKind>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheEntry>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheQuery>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheOperation>();
    let _ = std::any::TypeId::of::<boa_sw_core::CacheBatchReport>();
}
