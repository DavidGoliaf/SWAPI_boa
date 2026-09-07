//! Registration/worker registry: indexes and lookups (TS §6.5).
//!
//! `Registry` owns all records and exposes them only through methods: no public mutable fields,
//! no mutable iterators. Slot mutation (`installing`/`waiting`/`active`) happens exclusively
//! through `insert_*`/`replace_*`/`remove_*`, so later lifecycle tasks (`T-05`) can preserve the
//! invariants checked by [`Registry::check_invariants`].

use std::collections::HashMap;

use indexmap::IndexMap;
use url::Url;

use crate::ids::{RegistrationId, WorkerId};
use crate::key::StorageKey;
use crate::model::{RegistrationRecord, WorkerRecord};
use crate::url_util::{scope_matches, serialize_exclude_fragment};

/// Registration and worker indexes (TS §6.5).
///
/// The scope index key is `(storage key, fragment-free serialized scope with query preserved)`.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Lookup by `(storage key, normalized serialized scope)`, in insertion order.
    pub(crate) by_scope: IndexMap<(StorageKey, String), RegistrationId>,
    /// Registration records by id.
    pub(crate) registrations: HashMap<RegistrationId, RegistrationRecord>,
    /// Worker records by id.
    pub(crate) workers: HashMap<WorkerId, WorkerRecord>,
}

impl Registry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalizes `scope` for indexing: fragment-free serialization, query preserved (`R6.1.3`).
    ///
    /// [`serialize_exclude_fragment`] never fails: it clones `scope` and clears the fragment.
    fn scope_key(key: &StorageKey, scope: &Url) -> (StorageKey, String) {
        (key.clone(), serialize_exclude_fragment(scope))
    }

    /// Inserts a registration. Strips a fragment from the stored scope before indexing (`R6.1.3`).
    ///
    /// # Errors
    /// `Err` on a duplicate registration id or a duplicate `(storage key, scope)` index key.
    pub fn insert_registration(&mut self, mut record: RegistrationRecord) -> Result<(), String> {
        if self.registrations.contains_key(&record.id) {
            return Err(format!("duplicate registration id {}", record.id));
        }
        record.scope.set_fragment(None);
        let index_key = Self::scope_key(&record.storage_key, &record.scope);
        if self.by_scope.contains_key(&index_key) {
            return Err(format!(
                "duplicate registration for scope {}",
                record.scope.as_str()
            ));
        }
        // §6 rule 5: slots must reference existing workers.
        for slot in [record.installing, record.waiting, record.active]
            .into_iter()
            .flatten()
        {
            if !self.workers.contains_key(&slot) {
                return Err(format!(
                    "registration slot references unknown worker {slot}"
                ));
            }
        }
        self.by_scope.insert(index_key, record.id);
        self.registrations.insert(record.id, record);
        Ok(())
    }

    /// Inserts a worker record.
    ///
    /// # Errors
    /// `Err` on a duplicate worker id or when the referenced registration does not exist.
    pub fn insert_worker(&mut self, record: WorkerRecord) -> Result<(), String> {
        if self.workers.contains_key(&record.id) {
            return Err(format!("duplicate worker id {}", record.id));
        }
        if !self.registrations.contains_key(&record.registration) {
            return Err(format!(
                "worker {} references unknown registration {}",
                record.id, record.registration
            ));
        }
        self.workers.insert(record.id, record);
        Ok(())
    }

    /// Replaces a registration record, preserving id and index identity.
    ///
    /// The replacement must keep the same `id`, `storage_key` and normalized `scope` (scope
    /// changes belong to unregister + register, not to replacement). Slots must reference
    /// existing workers. The replacement is atomic: on `Err` the stored record is unchanged.
    ///
    /// # Errors
    /// `Err` when the id is unknown, the key/scope changed, or a slot dangles.
    pub fn replace_registration(&mut self, mut record: RegistrationRecord) -> Result<(), String> {
        let stored = self
            .registrations
            .get(&record.id)
            .ok_or_else(|| format!("unknown registration id {}", record.id))?;
        record.scope.set_fragment(None);
        if record.storage_key != stored.storage_key
            || serialize_exclude_fragment(&record.scope)
                != serialize_exclude_fragment(&stored.scope)
        {
            return Err(format!(
                "replacement of registration {} changes its key or scope",
                record.id
            ));
        }
        for slot in [record.installing, record.waiting, record.active]
            .into_iter()
            .flatten()
        {
            if !self.workers.contains_key(&slot) {
                return Err(format!(
                    "registration slot references unknown worker {slot}"
                ));
            }
        }
        self.registrations.insert(record.id, record);
        Ok(())
    }

    /// Replaces a worker record, preserving id.
    ///
    /// The replacement must keep the same `id` and reference an existing registration. Atomic:
    /// on `Err` the stored record is unchanged.
    ///
    /// # Errors
    /// `Err` when the id is unknown or the registration reference dangles.
    pub fn replace_worker(&mut self, record: WorkerRecord) -> Result<(), String> {
        if !self.workers.contains_key(&record.id) {
            return Err(format!("unknown worker id {}", record.id));
        }
        if !self.registrations.contains_key(&record.registration) {
            return Err(format!(
                "worker {} references unknown registration {}",
                record.id, record.registration
            ));
        }
        self.workers.insert(record.id, record);
        Ok(())
    }

    /// Removes a registration. Fails while any worker slot is still occupied; on success the
    /// scope index entry is removed as well. Missing ids return `Ok(None)`.
    ///
    /// # Errors
    /// `Err` when a worker slot is still occupied.
    pub fn remove_registration(
        &mut self,
        id: RegistrationId,
    ) -> Result<Option<RegistrationRecord>, String> {
        let Some(record) = self.registrations.get(&id) else {
            return Ok(None);
        };
        if record.installing.is_some() || record.waiting.is_some() || record.active.is_some() {
            return Err(format!(
                "cannot remove registration {id} with occupied worker slots"
            ));
        }
        // Invariant: index key exists exactly when the record does (checked by `check_invariants`).
        let index_key = Self::scope_key(&record.storage_key, &record.scope);
        self.by_scope.shift_remove(&index_key);
        Ok(self.registrations.remove(&id))
    }

    /// Removes a worker record. Fails while a registration slot still references it. Missing ids
    /// return `Ok(None)`.
    ///
    /// # Errors
    /// `Err` when the worker is still referenced by a registration slot.
    pub fn remove_worker(&mut self, id: WorkerId) -> Result<Option<WorkerRecord>, String> {
        if !self.workers.contains_key(&id) {
            return Ok(None);
        }
        for record in self.registrations.values() {
            if record.installing == Some(id)
                || record.waiting == Some(id)
                || record.active == Some(id)
            {
                return Err(format!(
                    "cannot remove worker {id} still referenced by a slot"
                ));
            }
        }
        Ok(self.workers.remove(&id))
    }

    /// Returns the registration record for `id`, if present.
    #[must_use]
    pub fn registration(&self, id: RegistrationId) -> Option<&RegistrationRecord> {
        self.registrations.get(&id)
    }

    /// Returns the worker record for `id`, if present.
    #[must_use]
    pub fn worker(&self, id: WorkerId) -> Option<&WorkerRecord> {
        self.workers.get(&id)
    }

    /// Exact registration lookup by storage key and scope (`R6.1.3` normalization applies).
    #[must_use]
    pub fn get_registration(&self, key: &StorageKey, scope: &Url) -> Option<RegistrationId> {
        self.by_scope.get(&Self::scope_key(key, scope)).copied()
    }

    /// Scope-match lookup: longest matching non-`uninstalling` scope for `key` (TS §6.5,
    /// `R6.5.2`). Returns `None` when nothing matches.
    ///
    /// Passes stored scope URLs directly to [`scope_matches`] (Q-02 semantics); no second
    /// fragment stripping, no re-serialization, no path-segment boundary check.
    #[must_use]
    pub fn match_registration(&self, key: &StorageKey, client_url: &Url) -> Option<RegistrationId> {
        let mut best: Option<(usize, RegistrationId)> = None;
        for ((scope_key, scope_serialized), id) in &self.by_scope {
            if scope_key != key {
                continue;
            }
            let Some(record) = self.registrations.get(id) else {
                continue;
            };
            if record.uninstalling || !scope_matches(&record.scope, client_url) {
                continue;
            }
            let len = scope_serialized.len();
            if len > best.map_or(0, |(best_len, _)| best_len) {
                best = Some((len, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Newest worker: `installing`, else `waiting`, else `active` (`R6.5.1`).
    #[must_use]
    pub fn newest_worker(&self, reg: RegistrationId) -> Option<WorkerId> {
        let record = self.registrations.get(&reg)?;
        record.installing.or(record.waiting).or(record.active)
    }

    /// All registration ids for `key`, in registry insertion order.
    #[must_use]
    pub fn registrations_for_origin(&self, key: &StorageKey) -> Vec<RegistrationId> {
        self.by_scope
            .iter()
            .filter(|((scope_key, _), _)| scope_key == key)
            .map(|(_, id)| *id)
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;

    /// Builds a `RegistrationRecord` with `id`, `key` and `scope`, no workers.
    pub fn registration(id: u64, key: &str, scope: &str) -> RegistrationRecord {
        RegistrationRecord {
            id: RegistrationId::from_raw(id),
            storage_key: StorageKey::from_raw(key),
            scope: Url::parse(scope).unwrap(),
            update_via_cache: crate::model::UpdateViaCache::Imports,
            installing: None,
            waiting: None,
            active: None,
            last_update_check_ms: None,
            navigation_preload_enabled: false,
            navigation_preload_header: String::from("true"),
            uninstalling: false,
        }
    }

    /// Builds a `WorkerRecord` with `id` in `reg`.
    pub fn worker(id: u64, reg: u64) -> WorkerRecord {
        WorkerRecord {
            id: WorkerId::from_raw(id),
            registration: RegistrationId::from_raw(reg),
            script_url: Url::parse("https://example.com/sw.js").unwrap(),
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
}

#[cfg(test)]
mod tests {
    use super::test_util::{registration, worker};
    use super::*;
    use crate::ids::RegistrationId;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn get_registration_isolated_by_key() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://a.example",
                "https://a.example/app/",
            ))
            .unwrap();
        registry
            .insert_registration(registration(
                2,
                "https://b.example",
                "https://a.example/app/",
            ))
            .unwrap();

        // Same serialized path, different keys: each lookup returns only its own registration.
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://a.example"),
                &url("https://a.example/app/")
            ),
            Some(RegistrationId::from_raw(1))
        );
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://b.example"),
                &url("https://a.example/app/")
            ),
            Some(RegistrationId::from_raw(2))
        );
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://c.example"),
                &url("https://a.example/app/")
            ),
            None
        );
    }

    #[test]
    fn match_registration_longest_prefix_wins() {
        let key = StorageKey::from_raw("https://example.com");
        let mut registry = Registry::new();
        for (id, scope) in [
            (1, "https://example.com/"),
            (2, "https://example.com/foo"),
            (3, "https://example.com/foo/bar/"),
        ] {
            registry
                .insert_registration(registration(id, "https://example.com", scope))
                .unwrap();
        }

        assert_eq!(
            registry.match_registration(&key, &url("https://example.com/foo/bar/baz")),
            Some(RegistrationId::from_raw(3))
        );
        // Intentional R6.2.2 prefix behaviour: `/foo` matches `/foobar`.
        assert_eq!(
            registry.match_registration(&key, &url("https://example.com/foobar")),
            Some(RegistrationId::from_raw(2))
        );
        assert_eq!(
            registry.match_registration(&key, &url("https://other.example/x")),
            None
        );
    }

    #[test]
    fn match_registration_skips_uninstalling() {
        let key = StorageKey::from_raw("https://example.com");
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        let mut long = registration(2, "https://example.com", "https://example.com/app/");
        long.uninstalling = true;
        registry.insert_registration(long).unwrap();

        // Longest match is uninstalling: falls back to the shorter eligible registration.
        assert_eq!(
            registry.match_registration(&key, &url("https://example.com/app/x")),
            Some(RegistrationId::from_raw(1))
        );

        // None remains when every match is uninstalling.
        let mut registry = Registry::new();
        let mut only = registration(1, "https://example.com", "https://example.com/");
        only.uninstalling = true;
        registry.insert_registration(only).unwrap();
        assert_eq!(
            registry.match_registration(&key, &url("https://example.com/x")),
            None
        );
    }

    #[test]
    fn newest_worker_precedence() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        for id in [10, 11, 12] {
            registry.insert_worker(worker(id, 1)).unwrap();
        }
        let mut record = registration(1, "https://example.com", "https://example.com/");
        record.installing = Some(WorkerId::from_raw(10));
        record.waiting = Some(WorkerId::from_raw(11));
        record.active = Some(WorkerId::from_raw(12));
        registry.replace_registration(record).unwrap();

        let reg = RegistrationId::from_raw(1);
        assert_eq!(registry.newest_worker(reg), Some(WorkerId::from_raw(10)));
        // Clear installing -> waiting wins; clear waiting -> active wins; clear all -> None.
        let mut record = registry.registration(reg).unwrap().clone();
        record.installing = None;
        registry.replace_registration(record).unwrap();
        assert_eq!(registry.newest_worker(reg), Some(WorkerId::from_raw(11)));
        let mut record = registry.registration(reg).unwrap().clone();
        record.waiting = None;
        registry.replace_registration(record).unwrap();
        assert_eq!(registry.newest_worker(reg), Some(WorkerId::from_raw(12)));
        let mut record = registry.registration(reg).unwrap().clone();
        record.active = None;
        registry.replace_registration(record).unwrap();
        assert_eq!(registry.newest_worker(reg), None);
    }

    #[test]
    fn registrations_preserve_insertion_order() {
        let mut registry = Registry::new();
        for id in [1, 2, 3] {
            registry
                .insert_registration(registration(
                    id,
                    "https://example.com",
                    &format!("https://example.com/app{id}/"),
                ))
                .unwrap();
        }
        registry
            .insert_registration(registration(
                4,
                "https://other.example",
                "https://other.example/",
            ))
            .unwrap();

        assert_eq!(
            registry.registrations_for_origin(&StorageKey::from_raw("https://example.com")),
            vec![
                RegistrationId::from_raw(1),
                RegistrationId::from_raw(2),
                RegistrationId::from_raw(3),
            ]
        );
        assert_eq!(
            registry.registrations_for_origin(&StorageKey::from_raw("https://other.example")),
            vec![RegistrationId::from_raw(4)]
        );
    }

    #[test]
    fn replace_methods_preserve_indexes() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();

        // Valid slot mutation through replace: index identity stable.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.installing = Some(WorkerId::from_raw(10));
        registry.replace_registration(record).unwrap();
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/")
            ),
            Some(RegistrationId::from_raw(1))
        );

        // Invalid replacement (dangling slot) rejected without partial mutation.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.waiting = Some(WorkerId::from_raw(99));
        assert!(registry.replace_registration(record).is_err());
        assert_eq!(
            registry
                .registration(RegistrationId::from_raw(1))
                .unwrap()
                .waiting,
            None
        );

        // Scope change through replace rejected.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.scope = url("https://example.com/other/");
        assert!(registry.replace_registration(record).is_err());

        // Unknown ids rejected.
        assert!(
            registry
                .replace_registration(registration(
                    9,
                    "https://example.com",
                    "https://example.com/x/"
                ))
                .is_err()
        );
        assert!(registry.replace_worker(worker(9, 1)).is_err());
    }

    #[test]
    fn insert_rejects_duplicates_and_dangling_refs() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        // Duplicate id.
        assert!(
            registry
                .insert_registration(registration(
                    1,
                    "https://example.com",
                    "https://example.com/other/"
                ))
                .is_err()
        );
        // Duplicate (key, scope).
        assert!(
            registry
                .insert_registration(registration(
                    2,
                    "https://example.com",
                    "https://example.com/#frag"
                ))
                .is_err()
        );
        // Worker with unknown registration.
        assert!(registry.insert_worker(worker(10, 9)).is_err());
        // Duplicate worker id.
        registry.insert_worker(worker(10, 1)).unwrap();
        assert!(registry.insert_worker(worker(10, 1)).is_err());
    }

    #[test]
    fn remove_cleans_all_indexes() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();

        // Occupied slot blocks both removals.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.installing = Some(WorkerId::from_raw(10));
        registry.replace_registration(record).unwrap();
        assert!(registry.remove_worker(WorkerId::from_raw(10)).is_err());
        assert!(
            registry
                .remove_registration(RegistrationId::from_raw(1))
                .is_err()
        );

        // After slots are cleared both removals succeed and clean every index.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.installing = None;
        registry.replace_registration(record).unwrap();
        assert!(
            registry
                .remove_worker(WorkerId::from_raw(10))
                .unwrap()
                .is_some()
        );
        assert!(registry.worker(WorkerId::from_raw(10)).is_none());
        assert!(
            registry
                .remove_registration(RegistrationId::from_raw(1))
                .unwrap()
                .is_some()
        );
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/")
            ),
            None
        );
        assert!(
            registry
                .registrations_for_origin(&StorageKey::from_raw("https://example.com"))
                .is_empty()
        );
        assert!(registry.check_invariants().is_ok());
        // Missing ids return Ok(None).
        assert!(
            registry
                .remove_worker(WorkerId::from_raw(10))
                .unwrap()
                .is_none()
        );
        assert!(
            registry
                .remove_registration(RegistrationId::from_raw(1))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn scope_with_fragment_is_normalized_on_insert() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/app/#frag",
            ))
            .unwrap();
        let stored = registry.registration(RegistrationId::from_raw(1)).unwrap();
        assert_eq!(stored.scope.as_str(), "https://example.com/app/");
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/app/#other")
            ),
            Some(RegistrationId::from_raw(1))
        );
    }
}
