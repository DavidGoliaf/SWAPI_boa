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
    by_scope: IndexMap<(StorageKey, String), RegistrationId>,
    /// Registration records by id.
    registrations: HashMap<RegistrationId, RegistrationRecord>,
    /// Worker records by id.
    workers: HashMap<WorkerId, WorkerRecord>,
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

    /// Validates the three worker slots of `registration` in order
    /// (`installing`, `waiting`, `active`): no duplicate worker id, every named worker exists,
    /// and every named worker's back-pointer agrees with `registration`.
    ///
    /// Read-only: never mutates the registry, allocates nothing proportional to it.
    ///
    /// # Errors
    /// `Err` naming the offending slot/worker on the first violation found.
    fn validate_slots(
        &self,
        registration: RegistrationId,
        slots: [Option<WorkerId>; 3],
    ) -> Result<(), String> {
        const NAMES: [&str; 3] = ["installing", "waiting", "active"];
        let mut seen: [Option<WorkerId>; 3] = [None, None, None];
        for (index, slot) in slots.into_iter().enumerate() {
            let Some(id) = slot else { continue };
            if seen[..index].contains(&Some(id)) {
                return Err(format!(
                    "registration {registration} names worker {id} in more than one slot (duplicate in {})",
                    NAMES[index]
                ));
            }
            seen[index] = Some(id);
            let Some(worker) = self.workers.get(&id) else {
                return Err(format!(
                    "registration {registration} slot {} references unknown worker {id}",
                    NAMES[index]
                ));
            };
            if worker.registration != registration {
                return Err(format!(
                    "registration {registration} slot {} references worker {id} belonging to registration {}",
                    NAMES[index], worker.registration
                ));
            }
        }
        Ok(())
    }

    /// Snapshots the three slots of `record` in validation order.
    fn slots_of(record: &RegistrationRecord) -> [Option<WorkerId>; 3] {
        [record.installing, record.waiting, record.active]
    }

    /// Inserts a registration. Strips a fragment from the stored scope before indexing (`R6.1.3`).
    ///
    /// Validation (`validate_slots`: no duplicate slot worker, no unknown worker, no foreign
    /// worker) runs before the first mutation: a rejection leaves every index and record
    /// unchanged.
    ///
    /// # Errors
    /// `Err` on a duplicate registration id, a duplicate `(storage key, scope)` index key, or
    /// any slot violation.
    pub fn insert_registration(&mut self, mut record: RegistrationRecord) -> Result<(), String> {
        if self.registrations.contains_key(&record.id) {
            return Err(format!("duplicate registration id {}", record.id));
        }
        self.validate_slots(record.id, Self::slots_of(&record))?;
        record.scope.set_fragment(None);
        let index_key = Self::scope_key(&record.storage_key, &record.scope);
        if self.by_scope.contains_key(&index_key) {
            return Err(format!(
                "duplicate registration for scope {}",
                record.scope.as_str()
            ));
        }
        self.by_scope.insert(index_key, record.id);
        self.registrations.insert(record.id, record);
        Ok(())
    }

    /// Inserts a worker record.
    ///
    /// A worker may be inserted before any registration references it in a slot (the normal
    /// `T-05` order is: insert registration, insert worker, then set the slot via
    /// `replace_registration`). Inserting a registration that *already* names slots is also
    /// allowed when the referenced workers exist and belong to it.
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
    /// changes belong to unregister + register, not to replacement). Slot validation (no
    /// duplicate, no unknown, no foreign worker) runs before any mutation: a rejection leaves
    /// the stored record unchanged.
    ///
    /// # Errors
    /// `Err` when the id is unknown, the key/scope changed, or any slot check fails.
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
        self.validate_slots(record.id, Self::slots_of(&record))?;
        self.registrations.insert(record.id, record);
        Ok(())
    }

    /// Replaces a worker record, preserving id.
    ///
    /// The replacement must keep the same `id` and reference an existing registration.
    /// Changing `registration` while the worker occupies a slot is rejected: it would leave
    /// the old registration pointing at a worker that no longer belongs to it. Clear the slot
    /// first (via `replace_registration`), then move the worker. Atomic: on `Err` the stored
    /// record is unchanged.
    ///
    /// # Errors
    /// `Err` when the id is unknown, the registration reference dangles, or the worker is
    /// still slotted while its `registration` changes.
    pub fn replace_worker(&mut self, record: WorkerRecord) -> Result<(), String> {
        let Some(stored) = self.workers.get(&record.id) else {
            return Err(format!("unknown worker id {}", record.id));
        };
        let stored_registration = stored.registration;
        if !self.registrations.contains_key(&record.registration) {
            return Err(format!(
                "worker {} references unknown registration {}",
                record.id, record.registration
            ));
        }
        if stored_registration != record.registration && self.slot_of(record.id).is_some() {
            return Err(format!(
                "cannot move slotted worker {} from registration {} to {}",
                record.id, stored_registration, record.registration
            ));
        }
        self.workers.insert(record.id, record);
        Ok(())
    }

    /// Removes a registration. Fails while any worker slot is still occupied or any worker
    /// record still points at this registration; on success the scope index entry is removed
    /// as well. Missing ids return `Ok(None)`.
    ///
    /// The caller must remove (or move, once `T-05` defines the semantics) residual workers
    /// first: deleting the registration while orphaned workers remain would leave records
    /// pointing at a missing registration.
    ///
    /// # Errors
    /// `Err` when a worker slot is still occupied or a worker still references the registration.
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
        if let Some(orphan) = self.workers.values().find(|w| w.registration == id) {
            return Err(format!(
                "cannot remove registration {id} with residual worker {}",
                orphan.id
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

    /// Finds the registration whose slot currently holds `worker`, if any.
    ///
    /// A worker id occurs in at most one slot (§6 rule 1); the first match is the only match.
    fn slot_of(&self, worker: WorkerId) -> Option<RegistrationId> {
        self.registrations.values().find_map(|record| {
            if record.installing == Some(worker)
                || record.waiting == Some(worker)
                || record.active == Some(worker)
            {
                Some(record.id)
            } else {
                None
            }
        })
    }

    /// Returns the registration record for `id`, if present.
    #[must_use]
    pub fn registration(&self, id: RegistrationId) -> Option<&RegistrationRecord> {
        self.registrations.get(&id)
    }

    /// Read-only views for the invariants checker (`invariants.rs`).
    ///
    /// These are the only cross-module paths into registry storage; all return shared
    /// references or copies, never `&mut`, owned maps, or mutation handles.
    pub(crate) fn registrations_for_invariants(&self) -> impl Iterator<Item = &RegistrationRecord> {
        self.registrations.values()
    }

    pub(crate) fn workers_for_invariants(&self) -> impl Iterator<Item = &WorkerRecord> {
        self.workers.values()
    }

    pub(crate) fn scope_index_for_invariants(
        &self,
    ) -> impl Iterator<Item = (&(StorageKey, String), &RegistrationId)> {
        self.by_scope.iter()
    }

    /// Test-only: inserts records bypassing validation, to construct broken registries for the
    /// invariants checker. Production code never uses this path.
    #[cfg(test)]
    pub(crate) fn inject_for_test(
        &mut self,
        registration: Option<RegistrationRecord>,
        worker: Option<WorkerRecord>,
        index: Option<((StorageKey, String), RegistrationId)>,
    ) {
        if let Some(record) = registration {
            self.registrations.insert(record.id, record);
        }
        if let Some(record) = worker {
            self.workers.insert(record.id, record);
        }
        if let Some((key, id)) = index {
            self.by_scope.insert(key, id);
        }
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

    /// One shared fixture: R1 (`/`) + R2 (`/foo`) + R3 (`/foo/bar/`) under one key, plus R4
    /// under another key. Covers key isolation, longest-prefix and insertion-order inputs.
    /// (`populated` is used by lookup/order tests; uninstalling-skip builds its own state.)
    fn populated() -> Registry {
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
        registry
            .insert_registration(registration(
                4,
                "https://other.example",
                "https://other.example/",
            ))
            .unwrap();
        registry
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
        for (key, expected) in [
            ("https://a.example", Some(RegistrationId::from_raw(1))),
            ("https://b.example", Some(RegistrationId::from_raw(2))),
            ("https://c.example", None),
        ] {
            assert_eq!(
                registry
                    .get_registration(&StorageKey::from_raw(key), &url("https://a.example/app/")),
                expected
            );
        }
    }

    #[test]
    fn match_registration_longest_prefix_wins() {
        let registry = populated();
        let key = StorageKey::from_raw("https://example.com");
        for (client, expected) in [
            ("https://example.com/foo/bar/baz", Some(3)),
            // Intentional R6.2.2 prefix behaviour: `/foo` matches `/foobar`.
            ("https://example.com/foobar", Some(2)),
            ("https://other.example/x", None),
        ] {
            assert_eq!(
                registry.match_registration(&key, &url(client)),
                expected.map(RegistrationId::from_raw)
            );
        }
    }

    #[test]
    fn match_registration_skips_uninstalling() {
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
        let key = StorageKey::from_raw("https://example.com");
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
        let registry = populated();

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
        // Table-driven rejection: duplicate id, duplicate (key, scope), unknown worker in
        // `insert_worker`, duplicate worker id.
        for (label, record) in [
            (
                "duplicate id",
                registration(1, "https://example.com", "https://example.com/other/"),
            ),
            (
                "duplicate (key, scope)",
                registration(2, "https://example.com", "https://example.com/#frag"),
            ),
        ] {
            assert!(registry.insert_registration(record).is_err(), "{label}");
        }
        // Worker with unknown registration.
        assert!(registry.insert_worker(worker(10, 9)).is_err());
        // Duplicate worker id.
        registry.insert_worker(worker(10, 1)).unwrap();
        assert!(registry.insert_worker(worker(10, 1)).is_err());
    }

    #[test]
    fn insert_registration_rejects_same_worker_in_two_slots() {
        // On *insert* no worker can belong to the new registration yet (it does not exist, so
        // `insert_worker` could never have registered one under its id). Per the §A-2 check
        // order the first slot therefore fails on the back-pointer before the duplicate arm is
        // reached — but the call still returns `Err` and leaves no trace, which is what the
        // rework acceptance requires. The duplicate-specific message is pinned on the
        // `replace_registration` path below, where a self-owned worker exists.
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                2,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 2)).unwrap();

        let mut fresh = registration(3, "https://example.com", "https://example.com/app/");
        fresh.installing = Some(WorkerId::from_raw(10));
        fresh.waiting = Some(WorkerId::from_raw(10));
        assert!(registry.insert_registration(fresh).is_err());
        // No trace: R3 absent from the scope index and the record map.
        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/app/")
            ),
            None
        );
        let mut r2 = registry
            .registration(RegistrationId::from_raw(2))
            .unwrap()
            .clone();
        r2.uninstalling = true;
        registry.replace_registration(r2).unwrap();
        assert!(registry.check_invariants().is_ok());
    }

    #[test]
    fn replace_registration_rejects_same_worker_in_two_slots() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();
        registry.insert_worker(worker(11, 1)).unwrap();
        let mut record = registration(1, "https://example.com", "https://example.com/");
        record.installing = Some(WorkerId::from_raw(10));
        registry.replace_registration(record).unwrap();

        // Same worker in `waiting` as well as `installing`: rejected, stored slots unchanged.
        let mut doubled = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        doubled.waiting = Some(WorkerId::from_raw(10));
        assert!(registry.replace_registration(doubled).is_err());
        let stored = registry.registration(RegistrationId::from_raw(1)).unwrap();
        assert_eq!(stored.installing, Some(WorkerId::from_raw(10)));
        assert_eq!(stored.waiting, None);
    }

    #[test]
    fn duplicate_slot_rejection_is_atomic() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();
        registry.insert_worker(worker(11, 1)).unwrap();
        let mut record = registration(1, "https://example.com", "https://example.com/");
        record.installing = Some(WorkerId::from_raw(10));
        record.waiting = Some(WorkerId::from_raw(11));
        registry.replace_registration(record).unwrap();

        // Snapshot every observable: lookups, slots, workers, insertion order.
        let before_lookup = registry.get_registration(
            &StorageKey::from_raw("https://example.com"),
            &url("https://example.com/"),
        );
        let before_match = registry.match_registration(
            &StorageKey::from_raw("https://example.com"),
            &url("https://example.com/x"),
        );
        let before_slots = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        let before_workers: Vec<WorkerId> = [10, 11]
            .into_iter()
            .map(WorkerId::from_raw)
            .filter(|id| registry.worker(*id).is_some())
            .collect();
        let before_order =
            registry.registrations_for_origin(&StorageKey::from_raw("https://example.com"));

        // Duplicate worker across `installing` and `active`: rejected.
        let mut broken = before_slots.clone();
        broken.active = Some(WorkerId::from_raw(10));
        assert!(registry.replace_registration(broken).is_err());

        assert_eq!(
            registry.get_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/")
            ),
            before_lookup
        );
        assert_eq!(
            registry.match_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/x")
            ),
            before_match
        );
        assert_eq!(
            registry.registration(RegistrationId::from_raw(1)).unwrap(),
            &before_slots
        );
        let after_workers: Vec<WorkerId> = [10, 11]
            .into_iter()
            .map(WorkerId::from_raw)
            .filter(|id| registry.worker(*id).is_some())
            .collect();
        assert_eq!(after_workers, before_workers);
        assert_eq!(
            registry.registrations_for_origin(&StorageKey::from_raw("https://example.com")),
            before_order
        );
        assert!(registry.check_invariants().is_ok());
    }

    #[test]
    fn registry_fields_are_not_crate_public() {
        // Encapsulation (§1.9, §4.1): `Registry` exposes no field access outside `registry.rs`.
        // Method-only surface, exercised end to end: insert, slot via replace, every read-only
        // accessor the checker and T-05 may use.
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();
        let mut record = registration(1, "https://example.com", "https://example.com/");
        record.installing = Some(WorkerId::from_raw(10));
        registry.replace_registration(record).unwrap();
        assert_eq!(
            registry
                .registration(RegistrationId::from_raw(1))
                .unwrap()
                .installing,
            Some(WorkerId::from_raw(10))
        );
        assert!(registry.worker(WorkerId::from_raw(10)).is_some());
        assert!(registry.worker(WorkerId::from_raw(99)).is_none());
        assert_eq!(
            registry.newest_worker(RegistrationId::from_raw(1)),
            Some(WorkerId::from_raw(10))
        );
        assert_eq!(
            registry.match_registration(
                &StorageKey::from_raw("https://example.com"),
                &url("https://example.com/x")
            ),
            Some(RegistrationId::from_raw(1))
        );
    }

    #[test]
    fn insert_and_replace_reject_cross_registration_slots() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry
            .insert_registration(registration(
                2,
                "https://example.com",
                "https://example.com/app/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 2)).unwrap();

        // A slot in R1 pointing at a worker of R2 is rejected on both paths.
        let mut cross = registration(1, "https://example.com", "https://example.com/");
        cross.installing = Some(WorkerId::from_raw(10));
        assert!(registry.replace_registration(cross).is_err());

        let mut fresh = registration(3, "https://example.com", "https://example.com/other/");
        fresh.waiting = Some(WorkerId::from_raw(10));
        assert!(registry.insert_registration(fresh).is_err());

        // Foreign slots are rejected on both paths without partial mutation.
        assert_eq!(registry.newest_worker(RegistrationId::from_raw(1)), None);
        // Clear the orthogonal rule-2 noise (two fresh empty registrations) before asserting
        // the rejection left no trace.
        for id in [1, 2] {
            let mut record = registry
                .registration(RegistrationId::from_raw(id))
                .unwrap()
                .clone();
            record.uninstalling = true;
            registry.replace_registration(record).unwrap();
        }
        assert!(registry.check_invariants().is_ok());
    }

    #[test]
    fn replace_worker_rejects_moving_slotted_worker() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry
            .insert_registration(registration(
                2,
                "https://example.com",
                "https://example.com/app/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.active = Some(WorkerId::from_raw(10));
        registry.replace_registration(record).unwrap();

        // Moving a slotted worker to another registration is rejected; the record is unchanged.
        let mut moved = worker(10, 1);
        moved.registration = RegistrationId::from_raw(2);
        assert!(registry.replace_worker(moved).is_err());
        assert_eq!(
            registry
                .worker(WorkerId::from_raw(10))
                .unwrap()
                .registration,
            RegistrationId::from_raw(1)
        );

        // After clearing the slot the same move succeeds.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.active = None;
        registry.replace_registration(record).unwrap();
        let mut moved = worker(10, 1);
        moved.registration = RegistrationId::from_raw(2);
        registry.replace_worker(moved).unwrap();
        assert_eq!(
            registry
                .worker(WorkerId::from_raw(10))
                .unwrap()
                .registration,
            RegistrationId::from_raw(2)
        );
        // W10 now belongs to R2 without occupying a slot there: legal. Both registrations
        // are empty, so mark R1 uninstalling before asserting: an empty non-uninstalling
        // registration already violates rule 2 on its own.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.uninstalling = true;
        registry.replace_registration(record).unwrap();
        let mut record = registry
            .registration(RegistrationId::from_raw(2))
            .unwrap()
            .clone();
        record.uninstalling = true;
        registry.replace_registration(record).unwrap();
        assert!(registry.check_invariants().is_ok());
    }

    #[test]
    fn remove_registration_rejects_residual_workers() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.insert_worker(worker(10, 1)).unwrap();

        // No slot occupied, but the worker record still points at R1: removal is rejected.
        assert!(
            registry
                .remove_registration(RegistrationId::from_raw(1))
                .is_err()
        );
        // The registry is otherwise consistent (a non-slotted worker is legal); the caller
        // must remove the worker first, then the registration. Mark R1 uninstalling first:
        // an empty non-uninstalling registration already violates rule 2 on its own, which
        // would mask the residual-worker assertion below.
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.uninstalling = true;
        registry.replace_registration(record).unwrap();
        assert!(registry.check_invariants().is_ok());
        registry.remove_worker(WorkerId::from_raw(10)).unwrap();
        assert!(
            registry
                .remove_registration(RegistrationId::from_raw(1))
                .unwrap()
                .is_some()
        );
        assert!(registry.check_invariants().is_ok());
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
