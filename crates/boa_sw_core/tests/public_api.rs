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
