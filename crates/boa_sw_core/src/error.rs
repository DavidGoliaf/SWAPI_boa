//! Error types for `boa-sw` (TS §12, `AD-12`).
//!
//! The single internal error type [`SwError`] is deliberately **not** `#[non_exhaustive]`:
//! every consumer matches exhaustively, so adding a variant is a compile error until the
//! mapping in [`SwError::js_kind`] is extended too.

/// Result alias used across the crate.
pub type SwResult<T> = Result<T, SwError>;

/// The single internal error type of `boa-sw` (TS §12, `AD-12`).
///
/// Deliberately **not** `#[non_exhaustive]`: every consumer matches exhaustively, so adding a
/// variant is a compile error until the mapping in [`SwError::js_kind`] is extended too.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SwError {
    /// Invalid URL provided.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    /// Cross-origin operation is not allowed.
    #[error("cross-origin operation is not allowed")]
    CrossOrigin,
    /// Insecure context (not HTTPS, localhost, or loopback).
    #[error("insecure context")]
    InsecureContext,
    /// Path restriction violated.
    #[error("path restriction violated")]
    PathRestriction,
    /// Unsupported script MIME type.
    #[error("unsupported script MIME type: {0}")]
    BadScriptMime(String),
    /// Script fetch failed.
    #[error("script fetch failed: {0}")]
    ScriptFetch(String),
    /// Script was redirected (disallowed).
    #[error("script redirected")]
    ScriptRedirect,
    /// Script evaluation failed.
    #[error("script evaluation failed: {0}")]
    ScriptEval(String),
    /// Install failed.
    #[error("install failed: {0}")]
    InstallFailed(String),
    /// Registration is invalid or removed.
    #[error("registration is invalid or removed")]
    InvalidState,
    /// Operation aborted.
    #[error("operation aborted")]
    Aborted,
    /// Operation timed out.
    #[error("timed out")]
    TimedOut,
    /// Network error.
    #[error("network error: {0}")]
    Network(String),
    /// Storage quota exceeded.
    #[error("quota exceeded")]
    QuotaExceeded,
    /// Storage failure.
    #[error("storage failure: {0}")]
    Storage(String),
    /// Value cannot be cloned.
    #[error("value cannot be cloned")]
    DataClone,
    /// Operation not supported.
    #[error("not supported: {0}")]
    NotSupported(String),
    /// Globals already defined.
    #[error("globals already defined: {name}")]
    ConflictingGlobals {
        /// The name of the conflicting global.
        name: String,
    },
    /// Runtime already installed.
    #[error("runtime already installed")]
    AlreadyInstalled,
    /// Re-entrant pump.
    #[error("re-entrant pump")]
    Reentrant,
    /// Internal invariant violated.
    #[error("internal invariant violated: {0}")]
    Internal(String),
}

/// How an [`SwError`] surfaces to JavaScript (TS §12).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum JsErrorKind {
    /// A native `TypeError`.
    TypeError,
    /// A native `RangeError`.
    RangeError,
    /// A `DOMException` with this name.
    Dom(&'static str),
    /// Never reaches JavaScript; returned to the host as a `Result`.
    HostOnly,
}

impl SwError {
    /// Total mapping to the JavaScript surface. Exhaustive by construction — no wildcard arm.
    #[must_use]
    pub fn js_kind(&self) -> JsErrorKind {
        match self {
            Self::InvalidUrl(_)
            | Self::ScriptFetch(_)
            | Self::ScriptEval(_)
            | Self::InstallFailed(_)
            | Self::Network(_) => JsErrorKind::TypeError,
            Self::CrossOrigin
            | Self::InsecureContext
            | Self::PathRestriction
            | Self::BadScriptMime(_)
            | Self::ScriptRedirect => JsErrorKind::Dom("SecurityError"),
            Self::InvalidState => JsErrorKind::Dom("InvalidStateError"),
            Self::Aborted | Self::TimedOut => JsErrorKind::Dom("AbortError"),
            Self::QuotaExceeded => JsErrorKind::Dom("QuotaExceededError"),
            Self::Storage(_) | Self::Internal(_) => JsErrorKind::Dom("UnknownError"),
            Self::DataClone => JsErrorKind::Dom("DataCloneError"),
            Self::NotSupported(_) => JsErrorKind::Dom("NotSupportedError"),
            Self::ConflictingGlobals { .. } | Self::AlreadyInstalled | Self::Reentrant => {
                JsErrorKind::HostOnly
            }
        }
    }

    /// `Some(name)` when [`Self::js_kind`] is [`JsErrorKind::Dom`], `None` otherwise.
    #[must_use]
    pub fn dom_name(&self) -> Option<&'static str> {
        match self.js_kind() {
            JsErrorKind::Dom(name) => Some(name),
            _ => None,
        }
    }

    /// The message used when the error carries none of its own (TS §12 table).
    #[must_use]
    pub fn default_message(&self) -> String {
        match self {
            Self::InvalidUrl(_) => "Failed to parse URL".to_owned(),
            Self::CrossOrigin => {
                "The origin of the provided scriptURL does not match the current origin".to_owned()
            }
            Self::InsecureContext => {
                "Service workers are only available in secure contexts".to_owned()
            }
            Self::PathRestriction => {
                "The path of the provided scope is not under the max scope allowed".to_owned()
            }
            Self::BadScriptMime(_) => "The script has an unsupported MIME type".to_owned(),
            Self::ScriptFetch(_) => "Failed to fetch a service worker script".to_owned(),
            Self::ScriptRedirect => {
                "The script resource is behind a redirect, which is disallowed".to_owned()
            }
            Self::ScriptEval(_) => "Failed to evaluate the service worker script".to_owned(),
            Self::InstallFailed(_) => "The service worker installation failed".to_owned(),
            Self::InvalidState => "The registration is in an invalid state".to_owned(),
            Self::Aborted => "The operation was aborted".to_owned(),
            Self::TimedOut => "The operation timed out".to_owned(),
            Self::Network(_) => "A network error occurred".to_owned(),
            Self::QuotaExceeded => "The storage quota has been exceeded".to_owned(),
            Self::Storage(_) => "A storage error occurred".to_owned(),
            Self::DataClone => "The value could not be cloned".to_owned(),
            Self::NotSupported(_) => "The operation is not supported".to_owned(),
            Self::ConflictingGlobals { .. } => "A conflicting global is already defined".to_owned(),
            Self::AlreadyInstalled => "The runtime is already installed in this context".to_owned(),
            Self::Reentrant => "pump() was called re-entrantly".to_owned(),
            Self::Internal(_) => "An internal error occurred".to_owned(),
        }
    }
}

/// Storage-layer failures (TS §4.2.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    /// Entry not found.
    #[error("not found")]
    NotFound,
    /// Conflict (e.g. duplicate key).
    #[error("conflict: {0}")]
    Conflict(String),
    /// Storage quota exceeded.
    #[error("quota exceeded")]
    QuotaExceeded,
    /// Data is corrupt.
    #[error("data is corrupt: {0}")]
    Corrupt(String),
    /// Storage is locked by another process.
    #[error("storage is locked by another process")]
    Locked,
    /// I/O failure.
    #[error("I/O failure: {0}")]
    Io(String),
    /// Unsupported by this backend.
    #[error("unsupported by this backend: {0}")]
    Unsupported(String),
}

/// Result alias for storage operations (TS §4.2.4).
pub type StorageResult<T> = Result<T, StorageError>;

/// Failures of a network fetch, as seen by the runtime and the host (TS §4.2.2, §8.2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetworkError {
    /// Network request failed.
    #[error("network request failed: {0}")]
    Failed(String),
    /// Request timed out.
    #[error("request timed out")]
    Timeout,
    /// Request aborted.
    #[error("request aborted")]
    Aborted,
    /// Body exceeds the size limit.
    #[error("body of {actual} bytes exceeds the limit of {limit}")]
    BodyTooLarge {
        /// Actual body size in bytes.
        actual: u64,
        /// Size limit in bytes.
        limit: u64,
    },
    /// The fetch handler threw.
    #[error("the fetch handler threw")]
    HandlerThrew,
    /// The respondWith promise rejected.
    #[error("the respondWith promise rejected")]
    HandlerRejected,
    /// respondWith produced an invalid response.
    #[error("respondWith produced an invalid response: {0}")]
    InvalidResponse(String),
}

/// Failures while fetching a script resource (TS §6.4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptFetchError {
    /// Network error.
    #[error(transparent)]
    Network(#[from] NetworkError),
    /// Unexpected HTTP status.
    #[error("unexpected HTTP status {0}")]
    BadStatus(u16),
    /// Unsupported MIME type.
    #[error("unsupported MIME type: {0}")]
    BadMime(String),
    /// The script response is a redirect.
    #[error("the script response is a redirect")]
    Redirected,
    /// Script exceeds the size limit.
    #[error("script of {actual} bytes exceeds the limit of {limit}")]
    TooLarge {
        /// Actual script size in bytes.
        actual: u64,
        /// Size limit in bytes.
        limit: u64,
    },
    /// Too many imported scripts.
    #[error("{actual} imported scripts exceed the limit of {limit}")]
    TooManyImports {
        /// Actual number of imports.
        actual: u32,
        /// Import limit.
        limit: u32,
    },
}

/// Failures while evaluating a script (TS §11.4.5).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptEvalError {
    /// Parse error.
    #[error("parse error: {message}")]
    Parse {
        /// Error message.
        message: String,
    },
    /// Uncaught exception.
    #[error("uncaught exception: {message}")]
    Throw {
        /// Error message.
        message: String,
    },
    /// The worker was terminated during evaluation.
    #[error("the worker was terminated during evaluation")]
    Terminated,
}

impl From<StorageError> for SwError {
    fn from(err: StorageError) -> Self {
        match err {
            StorageError::QuotaExceeded => Self::QuotaExceeded,
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<NetworkError> for SwError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::Timeout => Self::TimedOut,
            NetworkError::Aborted => Self::Aborted,
            other => Self::Network(other.to_string()),
        }
    }
}

impl From<ScriptFetchError> for SwError {
    fn from(err: ScriptFetchError) -> Self {
        match err {
            ScriptFetchError::BadMime(m) => Self::BadScriptMime(m),
            ScriptFetchError::Redirected => Self::ScriptRedirect,
            ScriptFetchError::TooLarge { .. } | ScriptFetchError::TooManyImports { .. } => {
                Self::QuotaExceeded
            }
            other => Self::ScriptFetch(other.to_string()),
        }
    }
}

impl From<ScriptEvalError> for SwError {
    fn from(err: ScriptEvalError) -> Self {
        Self::ScriptEval(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All 21 variants of `SwError`, constructed literally.
    const ALL: [SwError; 21] = [
        SwError::InvalidUrl(String::new()),
        SwError::CrossOrigin,
        SwError::InsecureContext,
        SwError::PathRestriction,
        SwError::BadScriptMime(String::new()),
        SwError::ScriptFetch(String::new()),
        SwError::ScriptRedirect,
        SwError::ScriptEval(String::new()),
        SwError::InstallFailed(String::new()),
        SwError::InvalidState,
        SwError::Aborted,
        SwError::TimedOut,
        SwError::Network(String::new()),
        SwError::QuotaExceeded,
        SwError::Storage(String::new()),
        SwError::DataClone,
        SwError::NotSupported(String::new()),
        SwError::ConflictingGlobals {
            name: String::new(),
        },
        SwError::AlreadyInstalled,
        SwError::Reentrant,
        SwError::Internal(String::new()),
    ];

    #[test]
    fn js_kind_is_total() {
        for err in &ALL {
            // This match is exhaustive — no wildcard arm.
            // Adding a variant to SwError without extending this match will fail to compile.
            let expected = match err {
                SwError::InvalidUrl(_)
                | SwError::ScriptFetch(_)
                | SwError::ScriptEval(_)
                | SwError::InstallFailed(_)
                | SwError::Network(_) => JsErrorKind::TypeError,
                SwError::CrossOrigin
                | SwError::InsecureContext
                | SwError::PathRestriction
                | SwError::BadScriptMime(_)
                | SwError::ScriptRedirect => JsErrorKind::Dom("SecurityError"),
                SwError::InvalidState => JsErrorKind::Dom("InvalidStateError"),
                SwError::Aborted | SwError::TimedOut => JsErrorKind::Dom("AbortError"),
                SwError::QuotaExceeded => JsErrorKind::Dom("QuotaExceededError"),
                SwError::Storage(_) | SwError::Internal(_) => JsErrorKind::Dom("UnknownError"),
                SwError::DataClone => JsErrorKind::Dom("DataCloneError"),
                SwError::NotSupported(_) => JsErrorKind::Dom("NotSupportedError"),
                SwError::ConflictingGlobals { .. }
                | SwError::AlreadyInstalled
                | SwError::Reentrant => JsErrorKind::HostOnly,
            };
            // Checks the real function against the table, not just the duplicated match above.
            assert_eq!(err.js_kind(), expected, "js_kind mismatch for {err:?}");
        }
    }

    #[test]
    fn default_message_table() {
        let expected = [
            (SwError::InvalidUrl(String::new()), "Failed to parse URL"),
            (
                SwError::CrossOrigin,
                "The origin of the provided scriptURL does not match the current origin",
            ),
            (
                SwError::InsecureContext,
                "Service workers are only available in secure contexts",
            ),
            (
                SwError::PathRestriction,
                "The path of the provided scope is not under the max scope allowed",
            ),
            (
                SwError::BadScriptMime(String::new()),
                "The script has an unsupported MIME type",
            ),
            (
                SwError::ScriptFetch(String::new()),
                "Failed to fetch a service worker script",
            ),
            (
                SwError::ScriptRedirect,
                "The script resource is behind a redirect, which is disallowed",
            ),
            (
                SwError::ScriptEval(String::new()),
                "Failed to evaluate the service worker script",
            ),
            (
                SwError::InstallFailed(String::new()),
                "The service worker installation failed",
            ),
            (
                SwError::InvalidState,
                "The registration is in an invalid state",
            ),
            (SwError::Aborted, "The operation was aborted"),
            (SwError::TimedOut, "The operation timed out"),
            (SwError::Network(String::new()), "A network error occurred"),
            (
                SwError::QuotaExceeded,
                "The storage quota has been exceeded",
            ),
            (SwError::Storage(String::new()), "A storage error occurred"),
            (SwError::DataClone, "The value could not be cloned"),
            (
                SwError::NotSupported(String::new()),
                "The operation is not supported",
            ),
            (
                SwError::ConflictingGlobals {
                    name: String::new(),
                },
                "A conflicting global is already defined",
            ),
            (
                SwError::AlreadyInstalled,
                "The runtime is already installed in this context",
            ),
            (SwError::Reentrant, "pump() was called re-entrantly"),
            (
                SwError::Internal(String::new()),
                "An internal error occurred",
            ),
        ];

        for (err, expected_msg) in &expected {
            assert_eq!(err.default_message(), *expected_msg);
        }
    }

    #[test]
    fn dom_name_agrees_with_js_kind() {
        for err in &ALL {
            let js_kind = err.js_kind();
            let dom_name = err.dom_name();
            match js_kind {
                JsErrorKind::Dom(name) => {
                    assert_eq!(dom_name, Some(name));
                }
                _ => {
                    assert_eq!(dom_name, None);
                }
            }
        }
    }

    #[test]
    fn host_only_variants_have_no_dom_name() {
        let host_only = [
            SwError::ConflictingGlobals {
                name: String::new(),
            },
            SwError::AlreadyInstalled,
            SwError::Reentrant,
        ];
        for err in &host_only {
            assert_eq!(err.js_kind(), JsErrorKind::HostOnly);
            assert_eq!(err.dom_name(), None);
        }
    }

    #[test]
    fn from_storage_error() {
        // QuotaExceeded → SwError::QuotaExceeded
        assert_eq!(
            SwError::from(StorageError::QuotaExceeded),
            SwError::QuotaExceeded
        );
        // NotFound → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::NotFound),
            SwError::Storage(_)
        ));
        // Conflict → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::Conflict("test".to_owned())),
            SwError::Storage(_)
        ));
        // Corrupt → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::Corrupt("test".to_owned())),
            SwError::Storage(_)
        ));
        // Locked → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::Locked),
            SwError::Storage(_)
        ));
        // Io → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::Io("test".to_owned())),
            SwError::Storage(_)
        ));
        // Unsupported → Storage(...)
        assert!(matches!(
            SwError::from(StorageError::Unsupported("test".to_owned())),
            SwError::Storage(_)
        ));
    }

    #[test]
    fn from_network_error() {
        // Timeout → TimedOut
        assert_eq!(SwError::from(NetworkError::Timeout), SwError::TimedOut);
        // Aborted → Aborted
        assert_eq!(SwError::from(NetworkError::Aborted), SwError::Aborted);
        // Failed → Network(...)
        assert!(matches!(
            SwError::from(NetworkError::Failed("test".to_owned())),
            SwError::Network(_)
        ));
        // BodyTooLarge → Network(...)
        assert!(matches!(
            SwError::from(NetworkError::BodyTooLarge {
                actual: 100,
                limit: 50
            }),
            SwError::Network(_)
        ));
        // HandlerThrew → Network(...)
        assert!(matches!(
            SwError::from(NetworkError::HandlerThrew),
            SwError::Network(_)
        ));
        // HandlerRejected → Network(...)
        assert!(matches!(
            SwError::from(NetworkError::HandlerRejected),
            SwError::Network(_)
        ));
        // InvalidResponse → Network(...)
        assert!(matches!(
            SwError::from(NetworkError::InvalidResponse("test".to_owned())),
            SwError::Network(_)
        ));
    }

    #[test]
    fn from_script_fetch_error() {
        // BadMime → BadScriptMime
        assert_eq!(
            SwError::from(ScriptFetchError::BadMime("text/plain".to_owned())),
            SwError::BadScriptMime("text/plain".to_owned())
        );
        // Redirected → ScriptRedirect
        assert_eq!(
            SwError::from(ScriptFetchError::Redirected),
            SwError::ScriptRedirect
        );
        // TooLarge → QuotaExceeded
        assert_eq!(
            SwError::from(ScriptFetchError::TooLarge {
                actual: 100,
                limit: 50
            }),
            SwError::QuotaExceeded
        );
        // TooManyImports → QuotaExceeded
        assert_eq!(
            SwError::from(ScriptFetchError::TooManyImports {
                actual: 10,
                limit: 5
            }),
            SwError::QuotaExceeded
        );
        // BadStatus → ScriptFetch(...)
        assert!(matches!(
            SwError::from(ScriptFetchError::BadStatus(404)),
            SwError::ScriptFetch(_)
        ));
        // Network → ScriptFetch(...)
        assert!(matches!(
            SwError::from(ScriptFetchError::Network(NetworkError::Failed(
                "test".to_owned()
            ))),
            SwError::ScriptFetch(_)
        ));
    }

    #[test]
    fn from_script_eval_error() {
        // Parse → ScriptEval(...)
        assert!(matches!(
            SwError::from(ScriptEvalError::Parse {
                message: "test".to_owned()
            }),
            SwError::ScriptEval(_)
        ));
        // Throw → ScriptEval(...)
        assert!(matches!(
            SwError::from(ScriptEvalError::Throw {
                message: "test".to_owned()
            }),
            SwError::ScriptEval(_)
        ));
        // Terminated → ScriptEval(...)
        assert!(matches!(
            SwError::from(ScriptEvalError::Terminated),
            SwError::ScriptEval(_)
        ));
    }

    #[test]
    fn display_strings_are_stable() {
        let expected = [
            (SwError::InvalidUrl("x".to_owned()), "invalid URL: x"),
            (
                SwError::CrossOrigin,
                "cross-origin operation is not allowed",
            ),
            (SwError::InsecureContext, "insecure context"),
            (SwError::PathRestriction, "path restriction violated"),
            (
                SwError::BadScriptMime("x".to_owned()),
                "unsupported script MIME type: x",
            ),
            (
                SwError::ScriptFetch("x".to_owned()),
                "script fetch failed: x",
            ),
            (SwError::ScriptRedirect, "script redirected"),
            (
                SwError::ScriptEval("x".to_owned()),
                "script evaluation failed: x",
            ),
            (SwError::InstallFailed("x".to_owned()), "install failed: x"),
            (SwError::InvalidState, "registration is invalid or removed"),
            (SwError::Aborted, "operation aborted"),
            (SwError::TimedOut, "timed out"),
            (SwError::Network("x".to_owned()), "network error: x"),
            (SwError::QuotaExceeded, "quota exceeded"),
            (SwError::Storage("x".to_owned()), "storage failure: x"),
            (SwError::DataClone, "value cannot be cloned"),
            (SwError::NotSupported("x".to_owned()), "not supported: x"),
            (
                SwError::ConflictingGlobals {
                    name: "x".to_owned(),
                },
                "globals already defined: x",
            ),
            (SwError::AlreadyInstalled, "runtime already installed"),
            (SwError::Reentrant, "re-entrant pump"),
            (
                SwError::Internal("x".to_owned()),
                "internal invariant violated: x",
            ),
        ];

        for (err, expected_str) in &expected {
            assert_eq!(err.to_string(), *expected_str);
        }
    }
}
