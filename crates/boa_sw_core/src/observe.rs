//! Observer hook (TS §4.2.5). Implementations MUST NOT affect behaviour.

/// Non-fatal conditions worth surfacing to the host (TS `R2.4.2`, `R4.2.9`).
#[non_exhaustive]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum WarningCode {
    /// The selected storage backend does not persist across process restarts.
    NonPersistentBackend,
    /// A pre-existing global was replaced because the host allowed it.
    GlobalOverwritten,
}

/// Everything the runtime reports to the host for logging, metrics and tests.
///
/// `#[non_exhaustive]` per `R4.2.11`: later tasks (`T-05`, `T-07`, `T-13`, `T-18`, `T-23`) add
/// variants. Host code matches with a wildcard arm; crate-internal code must not.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ObserverEvent<'a> {
    /// The runtime was installed into a context for this storage key.
    RuntimeInstalled {
        /// The storage key of the installed runtime.
        storage_key: &'a str,
    },
    /// A non-fatal condition.
    Warning {
        /// The warning code.
        code: WarningCode,
        /// Human-readable detail.
        detail: &'a str,
    },
    /// The host broke a documented contract (TS `R6.1.2a`, `R8.4.1`).
    HostContractViolation {
        /// Human-readable detail.
        detail: &'a str,
    },
    /// A storage batch was applied durably.
    StorageBatchApplied {
        /// Number of operations in the batch.
        ops: usize,
        /// Total bytes written.
        bytes: u64,
    },
}

/// Observer hook (TS §4.2.5). Implementations MUST NOT affect behaviour.
pub trait SwObserver: 'static {
    /// Called when an event occurs.
    fn on_event(&self, event: &ObserverEvent<'_>);
}

/// An observer that discards everything.
#[derive(Copy, Clone, Debug, Default)]
pub struct NullObserver;

impl SwObserver for NullObserver {
    fn on_event(&self, _event: &ObserverEvent<'_>) {
        // Intentionally empty.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_observer_is_inert() {
        let observer = NullObserver;
        // Constructing and calling `on_event` for each variant compiles and does nothing.
        observer.on_event(&ObserverEvent::RuntimeInstalled {
            storage_key: "test",
        });
        observer.on_event(&ObserverEvent::Warning {
            code: WarningCode::NonPersistentBackend,
            detail: "test",
        });
        observer.on_event(&ObserverEvent::HostContractViolation { detail: "test" });
        observer.on_event(&ObserverEvent::StorageBatchApplied { ops: 1, bytes: 100 });
    }
}
