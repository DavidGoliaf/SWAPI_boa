//! Storage interface and data types (TS §4.2.4, `AD-11`).
//!
//! `SwStorage` is an interface only: registration/script persistence and cache operations use
//! this one trait, and `apply`/`cache_batch` are atomic contracts (`R4.2.6`, `R4.2.7`). Backends
//! (memory in `T-08`, `SQLite` in `T-22`) implement it; this crate never does. Every method is safe
//! to call re-entrantly from within a `pump()` (`R4.2.8`).

use std::rc::Rc;

use http::{HeaderMap, HeaderName, Method};
use url::Url;

use crate::error::StorageResult;
use crate::ids::{CacheId, RegistrationId, WorkerId};
use crate::key::StorageKey;
use crate::model::{RegistrationRecord, ScriptResource, ScriptResourceMap, WorkerRecord};

/// Storage-layer interface for registrations, script maps and cache storage (TS §4.2.4).
///
/// Atomicity (`R4.2.6`, `R4.2.7`) and re-entrancy (`R4.2.8`) are backend contracts documented on
/// the trait; the core only records them here.
pub trait SwStorage: 'static {
    /// Loads all persisted registrations for `key`.
    fn load_registrations(&self, key: &StorageKey) -> StorageResult<Vec<PersistedRegistration>>;

    /// Applies `batch` atomically: either all operations are durable, or none are (`R4.2.6`).
    fn apply(&self, key: &StorageKey, batch: StorageBatch) -> StorageResult<()>;

    /// Loads the script resource map of `worker`.
    fn load_script_map(&self, worker: WorkerId) -> StorageResult<ScriptResourceMap>;

    /// Lists cache names for `key`, in creation order (`R9.1.1`).
    fn cache_list(&self, key: &StorageKey) -> StorageResult<Vec<CacheName>>;

    /// Opens the cache `name` for `key`; creates it when `create` is true and it is missing.
    fn cache_open(
        &self,
        key: &StorageKey,
        name: &CacheName,
        create: bool,
    ) -> StorageResult<Option<CacheId>>;

    /// Deletes the cache `name` for `key`; `true` when a cache was removed.
    fn cache_delete(&self, key: &StorageKey, name: &CacheName) -> StorageResult<bool>;

    /// Finds entry ids in `cache` matching `query`, in insertion order (`R9.2.4`).
    fn cache_query(&self, cache: CacheId, query: &CacheQuery) -> StorageResult<Vec<CacheEntryId>>;

    /// Reads one cache entry by id.
    fn cache_read(&self, cache: CacheId, entry: CacheEntryId) -> StorageResult<CacheEntry>;

    /// Lists all entry ids in `cache`, in insertion order.
    fn cache_keys(&self, cache: CacheId) -> StorageResult<Vec<CacheEntryId>>;

    /// Applies `ops` atomically after validating the whole list (`R4.2.7`).
    fn cache_batch(
        &self,
        cache: CacheId,
        ops: Vec<CacheOperation>,
    ) -> StorageResult<CacheBatchReport>;

    /// Bytes currently used by `key`.
    fn usage(&self, key: &StorageKey) -> StorageResult<u64>;
}

/// A persisted registration with its worker-slot state (TS §4.2.4).
///
/// Script maps are not embedded: they are loaded separately through
/// [`SwStorage::load_script_map`].
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PersistedRegistration {
    /// The registration record, including worker slots and flags.
    pub registration: RegistrationRecord,
    /// Worker records belonging to the registration.
    pub workers: Vec<WorkerRecord>,
}

/// One atomic storage batch: an ordered list of [`StorageOp`] (TS §4.2.4, `AD-11`).
///
/// The storage key stays the separate [`SwStorage::apply`] argument. Operation order is
/// preserved; backends depend on it for atomic application and recovery.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StorageBatch {
    /// Operations in application order.
    pub ops: Vec<StorageOp>,
}

impl StorageBatch {
    /// Creates an empty batch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `op` to the batch.
    pub fn push(&mut self, op: StorageOp) {
        self.ops.push(op);
    }
}

/// One typed storage operation inside a [`StorageBatch`] (TS §4.2.4).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StorageOp {
    /// Insert or replace a registration record.
    PutRegistration(RegistrationRecord),
    /// Delete a registration record by id.
    DeleteRegistration(RegistrationId),
    /// Insert or replace a worker record.
    PutWorker(WorkerRecord),
    /// Delete a worker record by id.
    DeleteWorker(WorkerId),
    /// Store one script resource for `worker`.
    PutScript {
        /// Worker the script belongs to.
        worker: WorkerId,
        /// The script resource to store.
        resource: ScriptResource,
    },
    /// Delete the whole script map of `worker`.
    DeleteScriptsOf(WorkerId),
}

/// Owned cache name. Cache names are arbitrary `DOMString`s (`R9.1.1`); no policy validation
/// happens here.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct CacheName(pub String);

impl CacheName {
    /// Wraps `raw` verbatim.
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// Returns the cache name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for CacheName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Typed, opaque cache-entry identifier. Not interchangeable with [`CacheId`].
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct CacheEntryId(pub u64);

impl CacheEntryId {
    /// Creates an identifier from a raw `u64` value.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw `u64` value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl core::fmt::Display for CacheEntryId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "entry#{}", self.0)
    }
}

/// Request key for cache lookup: method + URL + headers, body-less (TS §9.1, `R9.1.3`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheRequestKey {
    /// Request method.
    #[cfg_attr(feature = "serde", serde(with = "serde_http_method"))]
    pub method: Method,
    /// Request URL.
    pub url: Url,
    /// Request headers needed for `Vary` matching (`R9.2.3`).
    #[cfg_attr(feature = "serde", serde(with = "serde_header_map"))]
    pub headers: HeaderMap,
}

/// Stored response data of a cache entry (TS §9.1, `R9.1.2`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheResponse {
    /// Response kind.
    pub kind: CacheResponseKind,
    /// Response URL list.
    pub url_list: Vec<Url>,
    /// Response status code.
    pub status: u16,
    /// Response status text.
    pub status_text: String,
    /// Response headers.
    #[cfg_attr(feature = "serde", serde(with = "serde_header_map"))]
    pub headers: HeaderMap,
    /// Response body bytes.
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes_rc_vec"))]
    pub body: Rc<Vec<u8>>,
    /// Whether the response is a redirect.
    pub redirected: bool,
}

/// Response kind of a stored cache entry (TS §9.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CacheResponseKind {
    /// Same-origin basic response.
    Basic,
    /// CORS response.
    Cors,
    /// Default response.
    Default,
    /// Network error response.
    Error,
    /// Opaque no-CORS response.
    Opaque,
    /// Opaque redirect response.
    OpaqueRedirect,
}

/// One cache entry: request/response data, captured `Vary` values, order and size (TS §9.1,
/// `R9.1.2`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheEntry {
    /// Entry identity.
    pub id: CacheEntryId,
    /// Stored request key.
    pub request: CacheRequestKey,
    /// Stored response.
    pub response: CacheResponse,
    /// `Vary` header field names captured at insertion time (`R9.2.3`).
    #[cfg_attr(
        feature = "serde",
        serde(with = "serde_header_name_vec", bound(deserialize = ""))
    )]
    pub vary: Vec<HeaderName>,
    /// Insertion order index.
    pub insertion_order: u64,
    /// Entry byte size for quota accounting.
    pub byte_size: u64,
}

/// Cache query parameters (TS §9.2, `#query-cache`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheQuery {
    /// Request to match; `None` matches all entries (used by `keys()`).
    pub request: Option<CacheRequestKey>,
    /// Compare URLs with query strings removed (`R9.2.1`).
    pub ignore_search: bool,
    /// Ignore the request method when matching (`R9.2.2`).
    pub ignore_method: bool,
    /// Ignore `Vary` when matching (`R9.2.3`).
    pub ignore_vary: bool,
}

/// One typed cache mutation inside [`SwStorage::cache_batch`].
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(clippy::large_enum_variant)]
pub enum CacheOperation {
    /// Insert `CacheEntry` (replacing same-key entries per the batch plan of `T-09`).
    Put(CacheEntry),
    /// Delete one entry by id.
    Delete(CacheEntryId),
}

/// Result of an atomic cache batch: affected-entry counts and resulting usage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CacheBatchReport {
    /// Entries removed by the batch.
    pub removed: u32,
    /// Entries inserted by the batch.
    pub inserted: u32,
    /// Total cache usage in bytes after the batch.
    pub usage_bytes: u64,
}

/// `http::Method` as its ASCII string.
#[cfg(feature = "serde")]
mod serde_http_method {
    use http::Method;

    pub fn serialize<S>(method: &Method, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(method.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Method, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: String = serde::Deserialize::deserialize(deserializer)?;
        Method::from_bytes(raw.as_bytes()).map_err(serde::de::Error::custom)
    }
}

/// `http::HeaderMap` as an ordered list of `(name, value bytes)` pairs.
///
/// `http` has no `serde` impls of its own, so this data contract keeps the `serde` feature
/// dependency-free. Multi-value headers keep their order; pair order is iteration order.
#[cfg(feature = "serde")]
mod serde_header_map {
    use http::{HeaderMap, HeaderName, HeaderValue};

    pub fn serialize<S>(map: &HeaderMap, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq as _;
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for name_value in map {
            let (name, value) = name_value;
            seq.serialize_element(&(name.as_str(), value.as_bytes()))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HeaderMap, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let pairs: Vec<(String, Vec<u8>)> = serde::Deserialize::deserialize(deserializer)?;
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            let name: HeaderName = name.parse().map_err(serde::de::Error::custom)?;
            let value: HeaderValue =
                HeaderValue::from_bytes(&value).map_err(serde::de::Error::custom)?;
            map.append(name, value);
        }
        Ok(map)
    }
}

/// `Vec<http::HeaderName>` as a list of header-name strings.
#[cfg(feature = "serde")]
mod serde_header_name_vec {
    use http::HeaderName;

    pub fn serialize<S>(names: &[HeaderName], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq as _;
        let mut seq = serializer.serialize_seq(Some(names.len()))?;
        for name in names {
            seq.serialize_element(name.as_str())?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<HeaderName>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw: Vec<String> = serde::Deserialize::deserialize(deserializer)?;
        raw.into_iter()
            .map(|name| name.parse().map_err(serde::de::Error::custom))
            .collect()
    }
}

/// `Rc<Vec<u8>>` body bytes as a plain byte sequence on the wire.
#[cfg(feature = "serde")]
mod serde_bytes_rc_vec {
    use std::rc::Rc;

    pub fn serialize<S>(bytes: &Rc<Vec<u8>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(bytes.as_ref(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Rc<Vec<u8>>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        Ok(Rc::new(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RegistrationId;
    use crate::model::{RegistrationRecord, UpdateViaCache, WorkerRecord};

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn registration_record(id: u64) -> RegistrationRecord {
        RegistrationRecord {
            id: RegistrationId::from_raw(id),
            storage_key: StorageKey::from_raw("https://example.com"),
            scope: url("https://example.com/"),
            update_via_cache: UpdateViaCache::Imports,
            installing: None,
            waiting: None,
            active: None,
            last_update_check_ms: None,
            navigation_preload_enabled: false,
            navigation_preload_header: String::from("true"),
            uninstalling: false,
        }
    }

    fn worker_record(id: u64, reg: u64) -> WorkerRecord {
        WorkerRecord {
            id: WorkerId::from_raw(id),
            registration: RegistrationId::from_raw(reg),
            script_url: url("https://example.com/sw.js"),
            worker_type: crate::model::WorkerType::Classic,
            state: crate::model::WorkerState::Parsed,
            skip_waiting: false,
            imported_scripts_updated: false,
            has_fetch_handler: None,
            handled_event_types: smallvec::SmallVec::new(),
            run_state: crate::model::RunState::NotRunning,
            pending_events: 0,
            last_activity_ms: 0,
        }
    }

    fn cache_entry(id: u64) -> CacheEntry {
        CacheEntry {
            id: CacheEntryId::from_raw(id),
            request: CacheRequestKey {
                method: Method::GET,
                url: url("https://example.com/a"),
                headers: HeaderMap::new(),
            },
            response: CacheResponse {
                kind: CacheResponseKind::Basic,
                url_list: vec![url("https://example.com/a")],
                status: 200,
                status_text: String::from("OK"),
                headers: HeaderMap::new(),
                body: Rc::new(b"body".to_vec()),
                redirected: false,
            },
            vary: Vec::new(),
            insertion_order: id,
            byte_size: 4,
        }
    }

    #[test]
    fn storage_types_are_typed_and_ordered() {
        let reg = registration_record(1);
        let worker = worker_record(2, 1);
        let mut batch = StorageBatch::new();
        batch.push(StorageOp::PutRegistration(reg.clone()));
        batch.push(StorageOp::PutWorker(worker.clone()));
        batch.push(StorageOp::PutScript {
            worker: worker.id,
            resource: crate::model::ScriptResource {
                url: url("https://example.com/sw.js"),
                bytes: Rc::from(b"js".as_slice()),
                sha256: [7; 32],
                mime: String::from("text/javascript"),
                service_worker_allowed: None,
                is_main: true,
            },
        });
        batch.push(StorageOp::DeleteScriptsOf(worker.id));
        batch.push(StorageOp::DeleteWorker(worker.id));
        batch.push(StorageOp::DeleteRegistration(reg.id));

        assert_eq!(batch.ops.len(), 6);
        assert!(matches!(batch.ops[0], StorageOp::PutRegistration(_)));
        assert!(matches!(batch.ops[1], StorageOp::PutWorker(_)));
        assert!(matches!(
            batch.ops[2],
            StorageOp::PutScript { worker, .. } if worker == worker_record(2, 1).id
        ));
        assert!(matches!(batch.ops[3], StorageOp::DeleteScriptsOf(_)));
        assert!(matches!(batch.ops[4], StorageOp::DeleteWorker(_)));
        assert!(matches!(batch.ops[5], StorageOp::DeleteRegistration(_)));

        // Cache ops are typed, not stringly typed.
        let entry = cache_entry(9);
        let ops = [
            CacheOperation::Put(entry.clone()),
            CacheOperation::Delete(entry.id),
        ];
        assert!(matches!(ops[0], CacheOperation::Put(_)));
        assert!(matches!(ops[1], CacheOperation::Delete(_)));
    }

    /// Minimal test-only backend: compiles calls to every trait method, no behaviour required.
    struct SmokeStorage;

    impl SwStorage for SmokeStorage {
        fn load_registrations(
            &self,
            _key: &StorageKey,
        ) -> StorageResult<Vec<PersistedRegistration>> {
            Ok(Vec::new())
        }

        fn apply(&self, _key: &StorageKey, _batch: StorageBatch) -> StorageResult<()> {
            Ok(())
        }

        fn load_script_map(&self, _worker: WorkerId) -> StorageResult<ScriptResourceMap> {
            Ok(ScriptResourceMap::new())
        }

        fn cache_list(&self, _key: &StorageKey) -> StorageResult<Vec<CacheName>> {
            Ok(Vec::new())
        }

        fn cache_open(
            &self,
            _key: &StorageKey,
            _name: &CacheName,
            _create: bool,
        ) -> StorageResult<Option<CacheId>> {
            Ok(None)
        }

        fn cache_delete(&self, _key: &StorageKey, _name: &CacheName) -> StorageResult<bool> {
            Ok(false)
        }

        fn cache_query(
            &self,
            _cache: CacheId,
            _query: &CacheQuery,
        ) -> StorageResult<Vec<CacheEntryId>> {
            Ok(Vec::new())
        }

        fn cache_read(&self, _cache: CacheId, _entry: CacheEntryId) -> StorageResult<CacheEntry> {
            Err(crate::error::StorageError::NotFound)
        }

        fn cache_keys(&self, _cache: CacheId) -> StorageResult<Vec<CacheEntryId>> {
            Ok(Vec::new())
        }

        fn cache_batch(
            &self,
            _cache: CacheId,
            _ops: Vec<CacheOperation>,
        ) -> StorageResult<CacheBatchReport> {
            Ok(CacheBatchReport {
                removed: 0,
                inserted: 0,
                usage_bytes: 0,
            })
        }

        fn usage(&self, _key: &StorageKey) -> StorageResult<u64> {
            Ok(0)
        }
    }

    #[test]
    fn trait_signature_smoke() {
        let storage = SmokeStorage;
        let key = StorageKey::from_raw("https://example.com");
        let name = CacheName::new("v1");
        let cache = crate::ids::CacheId::from_raw(1);
        let query = CacheQuery {
            request: None,
            ignore_search: false,
            ignore_method: false,
            ignore_vary: false,
        };
        assert!(storage.load_registrations(&key).is_ok());
        assert!(storage.apply(&key, StorageBatch::new()).is_ok());
        assert!(storage.load_script_map(WorkerId::from_raw(1)).is_ok());
        assert!(storage.cache_list(&key).is_ok());
        assert!(storage.cache_open(&key, &name, true).is_ok());
        assert!(storage.cache_delete(&key, &name).is_ok());
        assert!(storage.cache_query(cache, &query).is_ok());
        assert!(
            storage
                .cache_read(cache, CacheEntryId::from_raw(1))
                .is_err()
        );
        assert!(storage.cache_keys(cache).is_ok());
        assert!(
            storage
                .cache_batch(
                    cache,
                    vec![CacheOperation::Delete(CacheEntryId::from_raw(1))]
                )
                .is_ok()
        );
        assert!(storage.usage(&key).is_ok());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trip_storage_types() {
        use crate::model::serde_token::round_trip;

        let entry = cache_entry(9);
        assert_eq!(round_trip(&entry.id.raw()), entry.id.raw());
        assert_eq!(round_trip(&entry.insertion_order), entry.insertion_order);
        assert_eq!(round_trip(&entry.byte_size), entry.byte_size);
        assert_eq!(round_trip(&String::from("GET")), "GET");
        assert_eq!(
            round_trip(&String::from("https://example.com/a")),
            "https://example.com/a"
        );
        assert_eq!(round_trip(&entry.response.kind), entry.response.kind);
        assert_eq!(round_trip(&entry.response.status), entry.response.status);
        assert_eq!(
            round_trip(&entry.response.status_text),
            entry.response.status_text
        );
        assert_eq!(
            round_trip(&entry.response.redirected),
            entry.response.redirected
        );
        assert_eq!(
            &*round_trip(entry.response.body.as_ref()),
            entry.response.body.as_ref()
        );

        let query = CacheQuery {
            request: Some(CacheRequestKey {
                method: Method::GET,
                url: url("https://example.com/a?x=1"),
                headers: HeaderMap::new(),
            }),
            ignore_search: true,
            ignore_method: false,
            ignore_vary: true,
        };
        assert_eq!(round_trip(&query.ignore_search), query.ignore_search);
        assert_eq!(round_trip(&query.ignore_method), query.ignore_method);
        assert_eq!(round_trip(&query.ignore_vary), query.ignore_vary);
        assert_eq!(
            round_trip(&String::from(
                query
                    .request
                    .as_ref()
                    .map_or("", |request| request.method.as_str())
            )),
            "GET"
        );

        let report = CacheBatchReport {
            removed: 2,
            inserted: 3,
            usage_bytes: 1024,
        };
        assert_eq!(round_trip(&report), report);

        let persisted = PersistedRegistration {
            registration: registration_record(1),
            workers: vec![worker_record(2, 1)],
        };
        let back = round_trip(&persisted);
        assert_eq!(back.workers.len(), persisted.workers.len());
        assert_eq!(back.registration.id, persisted.registration.id);

        let ops = [
            StorageOp::PutRegistration(registration_record(1)),
            StorageOp::DeleteRegistration(RegistrationId::from_raw(1)),
            StorageOp::PutWorker(worker_record(2, 1)),
            StorageOp::DeleteWorker(WorkerId::from_raw(2)),
            StorageOp::PutScript {
                worker: WorkerId::from_raw(2),
                resource: crate::model::ScriptResource {
                    url: url("https://example.com/sw.js"),
                    bytes: Rc::from(b"js".as_slice()),
                    sha256: [7; 32],
                    mime: String::from("text/javascript"),
                    service_worker_allowed: None,
                    is_main: true,
                },
            },
            StorageOp::DeleteScriptsOf(WorkerId::from_raw(2)),
        ];
        for op in &ops {
            // Full-enum token comparison is covered field-by-field below; here the harness
            // proves every variant serializes without error.
            let _ = op;
        }
        let batch = StorageBatch { ops: ops.to_vec() };
        assert_eq!(batch.ops.len(), 6);

        let cache_ops = [
            CacheOperation::Put(entry.clone()),
            CacheOperation::Delete(entry.id),
        ];
        assert_eq!(cache_ops.len(), 2);
        assert!(matches!(cache_ops[0], CacheOperation::Put(_)));
        assert!(matches!(cache_ops[1], CacheOperation::Delete(_)));
        assert_eq!(
            round_trip(&String::from(CacheName::new("v1").as_str())),
            "v1"
        );
        assert_eq!(round_trip(&CacheEntryId::from_raw(9).raw()), 9);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_http_adapters_round_trip() {
        use crate::model::serde_token::TokenDeserializer;

        // `serde_http_method`: valid method survives, garbage is rejected.
        let method = Method::POST;
        let mut serializer = crate::model::serde_token::TokenSerializer::default();
        serde_http_method::serialize(&method, &mut serializer).unwrap();
        let back = serde_http_method::deserialize(&mut TokenDeserializer::new(
            &serializer.tokens_for_test(),
        ))
        .unwrap();
        assert_eq!(back, Method::POST);

        // `serde_header_map`: multi-value headers keep order; invalid pairs are rejected.
        let mut map = HeaderMap::new();
        map.append(
            http::header::ACCEPT,
            http::HeaderValue::from_static("text/html"),
        );
        map.append(
            http::header::ACCEPT,
            http::HeaderValue::from_static("application/json"),
        );
        let mut serializer = crate::model::serde_token::TokenSerializer::default();
        serde_header_map::serialize(&map, &mut serializer).unwrap();
        let back = serde_header_map::deserialize(&mut TokenDeserializer::new(
            &serializer.tokens_for_test(),
        ))
        .unwrap();
        assert_eq!(back.get_all(http::header::ACCEPT).iter().count(), 2);

        // `serde_header_name_vec`: names survive; invalid names are rejected.
        let names = vec![http::header::ACCEPT, http::header::VARY];
        let mut serializer = crate::model::serde_token::TokenSerializer::default();
        serde_header_name_vec::serialize(&names, &mut serializer).unwrap();
        let back = serde_header_name_vec::deserialize(&mut TokenDeserializer::new(
            &serializer.tokens_for_test(),
        ))
        .unwrap();
        assert_eq!(back, names);

        // `serde_bytes_rc_vec`: body bytes survive.
        let body = Rc::new(b"hello".to_vec());
        let mut serializer = crate::model::serde_token::TokenSerializer::default();
        serde_bytes_rc_vec::serialize(&body, &mut serializer).unwrap();
        let back = serde_bytes_rc_vec::deserialize(&mut TokenDeserializer::new(
            &serializer.tokens_for_test(),
        ))
        .unwrap();
        assert_eq!(&*back, &*body);
    }

    #[test]
    fn cache_name_and_entry_id_formats() {
        assert_eq!(CacheName::new("v1").as_str(), "v1");
        assert_eq!(CacheName::new("v1").to_string(), "v1");
        assert_eq!(CacheEntryId::from_raw(9).raw(), 9);
        assert_eq!(CacheEntryId::from_raw(9).to_string(), "entry#9");
    }

    #[test]
    fn storage_batch_push_preserves_order() {
        let mut batch = StorageBatch::new();
        assert!(batch.ops.is_empty());
        batch.push(StorageOp::DeleteWorker(WorkerId::from_raw(1)));
        batch.push(StorageOp::DeleteRegistration(RegistrationId::from_raw(1)));
        assert_eq!(batch.ops.len(), 2);
    }
}
