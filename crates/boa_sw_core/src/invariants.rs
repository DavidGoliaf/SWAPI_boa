//! Invariants checker for the registry (`R15.5.3`).
//!
//! [`Registry::check_invariants`] is non-mutating, total (no panics), and returns a diagnostic
//! naming the violated rule. Debug builds and tests assert it after every step; `T-05` will call
//! it after every `SwCore::handle`.

use std::collections::HashSet;

use crate::ids::{RegistrationId, WorkerId};
use crate::model::{RunState, WorkerState};
use crate::registry::Registry;
use crate::url_util::serialize_exclude_fragment;

impl Registry {
    /// Checks every `R15.5.3` rule without mutating the registry.
    ///
    /// Rules (§6 of work order `T-04`, extended by review fixes):
    /// 1. no worker in two slots; 2. worker-less registration only when `uninstalling`;
    /// 3. no `Running` + `Redundant` worker; 4. `pending_events` never negative (by `u32`
    ///    construction; underflow made impossible through the mutation API);
    /// 5. ids unique, slots reference existing workers; 6. index key agrees with the record;
    /// 7. every worker points to an existing registration;
    /// 8. every slot references a worker whose back-pointer agrees (slot ↔ `registration`);
    /// 9. every registration record has a scope-index entry.
    ///
    /// # Errors
    /// `Err` with a diagnostic naming the violated rule.
    pub fn check_invariants(&self) -> Result<(), String> {
        self.check_slots_unique()?;
        self.check_workerless_implies_uninstalling()?;
        self.check_no_running_redundant()?;
        self.check_slots_reference_existing_workers()?;
        self.check_slots_agree_with_workers()?;
        self.check_index_keys_agree()?;
        self.check_every_registration_indexed()?;
        self.check_workers_point_to_registrations()?;
        Ok(())
    }

    /// §6 rule 1: a worker id occurs in at most one slot across all registrations.
    fn check_slots_unique(&self) -> Result<(), String> {
        let mut seen: HashSet<WorkerId> = HashSet::new();
        for record in self.registrations_for_invariants() {
            for slot in [record.installing, record.waiting, record.active]
                .into_iter()
                .flatten()
            {
                if !seen.insert(slot) {
                    return Err(format!(
                        "worker {slot} is present in more than one registration slot"
                    ));
                }
            }
        }
        Ok(())
    }

    /// §6 rule 2: a registration without workers is allowed only when `uninstalling`.
    fn check_workerless_implies_uninstalling(&self) -> Result<(), String> {
        for record in self.registrations_for_invariants() {
            if record.installing.is_none()
                && record.waiting.is_none()
                && record.active.is_none()
                && !record.uninstalling
            {
                return Err(format!(
                    "registration {} has no workers but is not uninstalling",
                    record.id
                ));
            }
        }
        Ok(())
    }

    /// §6 rule 3: no worker is `Running` while `Redundant`.
    fn check_no_running_redundant(&self) -> Result<(), String> {
        for worker in self.workers_for_invariants() {
            if worker.run_state == RunState::Running && worker.state == WorkerState::Redundant {
                return Err(format!("worker {} is Running and Redundant", worker.id));
            }
        }
        Ok(())
    }

    /// §6 rule 5 (part 1): every slot references an existing worker.
    fn check_slots_reference_existing_workers(&self) -> Result<(), String> {
        for record in self.registrations_for_invariants() {
            for slot in [record.installing, record.waiting, record.active]
                .into_iter()
                .flatten()
            {
                if self.worker(slot).is_none() {
                    return Err(format!(
                        "registration {} slot references missing worker {slot}",
                        record.id
                    ));
                }
            }
        }
        Ok(())
    }

    /// §6 rule 8: every slotted worker's back-pointer agrees with the slotting registration.
    ///
    /// `insert_*`/`replace_*` enforce this on write; the checker re-verifies it so a registry
    /// built by any other path (restore, backend) cannot silently diverge.
    fn check_slots_agree_with_workers(&self) -> Result<(), String> {
        for record in self.registrations_for_invariants() {
            for slot in [record.installing, record.waiting, record.active]
                .into_iter()
                .flatten()
            {
                let Some(worker) = self.worker(slot) else {
                    continue;
                };
                if worker.registration != record.id {
                    return Err(format!(
                        "registration {} slot holds worker {slot} belonging to registration {}",
                        record.id, worker.registration
                    ));
                }
            }
        }
        Ok(())
    }

    /// §6 rule 6: every scope-index entry points at a record whose id, key and normalized scope
    /// agree with the index key.
    fn check_index_keys_agree(&self) -> Result<(), String> {
        for ((key, scope_serialized), id) in self.scope_index_for_invariants() {
            let Some(record) = self.registration(*id) else {
                return Err(format!("scope index points at missing registration {id}"));
            };
            if record.id != *id {
                return Err(format!(
                    "scope index id {id} disagrees with record id {}",
                    record.id
                ));
            }
            if record.storage_key != *key {
                return Err(format!(
                    "registration {id} storage key disagrees with its scope index key"
                ));
            }
            if serialize_exclude_fragment(&record.scope) != *scope_serialized {
                return Err(format!(
                    "registration {id} scope disagrees with its scope index key"
                ));
            }
        }
        Ok(())
    }

    /// §6 rule 9: every registration record has a scope-index entry pointing at it.
    ///
    /// The forward direction (index → record) is `check_index_keys_agree`; this is the reverse
    /// (record → index). A record invisible to `get_registration`/`match_registration` but
    /// visible to `newest_worker` would split the registry's view of itself.
    fn check_every_registration_indexed(&self) -> Result<(), String> {
        for record in self.registrations_for_invariants() {
            let expected = (
                record.storage_key.clone(),
                serialize_exclude_fragment(&record.scope),
            );
            match self
                .scope_index_for_invariants()
                .find(|(key, _)| *key == &expected)
            {
                Some((_, indexed)) if *indexed == record.id => {}
                Some((_, indexed)) => {
                    return Err(format!(
                        "registration {} scope index points at registration {indexed}",
                        record.id
                    ));
                }
                None => {
                    return Err(format!(
                        "registration {} has no scope index entry",
                        record.id
                    ));
                }
            }
        }
        Ok(())
    }

    /// §6 rule 7: every worker record points at an existing registration.
    fn check_workers_point_to_registrations(&self) -> Result<(), String> {
        for worker in self.workers_for_invariants() {
            if self.registration(worker.registration).is_none() {
                return Err(format!(
                    "worker {} points at missing registration {}",
                    worker.id, worker.registration
                ));
            }
        }
        Ok(())
    }
}

/// Read-only internal views used by the checker.
impl Registry {
    fn registrations_for_invariants(
        &self,
    ) -> impl Iterator<Item = &crate::model::RegistrationRecord> {
        self.registrations.values()
    }

    fn workers_for_invariants(&self) -> impl Iterator<Item = &crate::model::WorkerRecord> {
        self.workers.values()
    }

    fn scope_index_for_invariants(
        &self,
    ) -> impl Iterator<Item = (&(crate::key::StorageKey, String), &RegistrationId)> {
        self.by_scope.iter()
    }

    /// Test-only: inserts a record bypassing validation, to construct broken registries.
    #[cfg(test)]
    pub(crate) fn inject_for_test(
        &mut self,
        registration: Option<crate::model::RegistrationRecord>,
        worker: Option<crate::model::WorkerRecord>,
        index: Option<((crate::key::StorageKey, String), RegistrationId)>,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::test_util::{registration, worker};
    use url::Url;

    fn valid_registry() -> Registry {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        registry.inject_for_test(None, Some(worker(10, 1)), None);
        let mut record = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        record.installing = Some(crate::ids::WorkerId::from_raw(10));
        registry.inject_for_test(Some(record), None, None);
        registry
    }

    #[test]
    fn valid_registry_passes() {
        assert!(valid_registry().check_invariants().is_ok());
    }

    #[test]
    fn detects_worker_in_two_slots() {
        let mut registry = valid_registry();
        let mut second = registration(2, "https://example.com", "https://example.com/app/");
        second.waiting = Some(crate::ids::WorkerId::from_raw(10));
        registry.inject_for_test(
            Some(second),
            None,
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/app/"),
                ),
                RegistrationId::from_raw(2),
            )),
        );
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("more than one registration slot"), "{err}");
    }

    #[test]
    fn detects_empty_non_uninstalling_registration() {
        let mut registry = Registry::new();
        registry
            .insert_registration(registration(
                1,
                "https://example.com",
                "https://example.com/",
            ))
            .unwrap();
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("has no workers"), "{err}");

        // Uninstalling with no workers is valid.
        let mut registry = Registry::new();
        let mut record = registration(1, "https://example.com", "https://example.com/");
        record.uninstalling = true;
        registry.insert_registration(record).unwrap();
        assert!(registry.check_invariants().is_ok());
    }

    #[test]
    fn detects_running_redundant_worker() {
        let mut registry = valid_registry();
        let mut broken = worker(10, 1);
        broken.run_state = RunState::Running;
        broken.state = WorkerState::Redundant;
        registry.inject_for_test(None, Some(broken), None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("Running and Redundant"), "{err}");
    }

    #[test]
    fn pending_events_cannot_underflow_by_construction() {
        // §6 rule 4: `pending_events` is `u32`; no mutation API exists that could decrement it
        // below zero, so underflow is impossible by construction. Saturating arithmetic is the
        // backstop `T-05` mutation helpers must use; this pins the type-level guarantee.
        let worker = worker(10, 1);
        assert_eq!(worker.pending_events, 0);
        assert_eq!(worker.pending_events.saturating_sub(1), 0);
    }

    #[test]
    fn detects_duplicate_or_dangling_ids() {
        // Dangling slot reference.
        let mut registry = valid_registry();
        let mut broken = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        broken.waiting = Some(crate::ids::WorkerId::from_raw(99));
        registry.inject_for_test(Some(broken), None, None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("missing worker"), "{err}");
    }

    #[test]
    fn detects_slot_worker_registration_mismatch() {
        // Slot in R1 holds a worker whose back-pointer says R2 (`inject_for_test` bypasses the
        // `insert_*`/`replace_*` validation that now rejects this on write). Both registrations
        // are `uninstalling` so rule 2 does not fire first; the back-pointer mismatch is the
        // reported violation.
        let mut registry = Registry::new();
        let mut r1 = registration(1, "https://example.com", "https://example.com/");
        r1.uninstalling = true;
        let mut r2 = registration(2, "https://example.com", "https://example.com/app/");
        r2.uninstalling = true;
        registry.inject_for_test(
            Some(r1),
            Some(worker(10, 2)),
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/"),
                ),
                RegistrationId::from_raw(1),
            )),
        );
        registry.inject_for_test(
            Some(r2),
            None,
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/app/"),
                ),
                RegistrationId::from_raw(2),
            )),
        );
        let mut broken = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        broken.installing = Some(crate::ids::WorkerId::from_raw(10));
        registry.inject_for_test(Some(broken), None, None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("belonging to registration"), "{err}");
    }

    #[test]
    fn detects_registration_missing_from_scope_index() {
        // Record present in `registrations` but with no scope-index entry: invisible to
        // `get_registration`/`match_registration` yet visible to `newest_worker`. R2 is
        // `uninstalling` so rule 2 does not fire first; the missing index entry is the
        // reported violation.
        let mut registry = valid_registry();
        let mut ghost = registration(2, "https://example.com", "https://example.com/app/");
        ghost.uninstalling = true;
        registry.inject_for_test(Some(ghost), None, None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("has no scope index entry"), "{err}");
    }

    #[test]
    fn detects_scope_index_pointing_at_wrong_registration() {
        // Two records share one scope string in the index (only one can own it): the loser is
        // reported as mis-indexed.
        let mut registry = valid_registry();
        registry.inject_for_test(
            None,
            None,
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/app/"),
                ),
                RegistrationId::from_raw(1),
            )),
        );
        let err = registry.check_invariants().unwrap_err();
        assert!(
            err.contains("scope disagrees with its scope index key"),
            "{err}"
        );
    }

    #[test]
    fn detects_dangling_scope_index_entry() {
        // Index entry points at a registration that was removed without cleaning the index.
        // `inject_for_test` inserts the extra index key verbatim (no validation), so the
        // checker must catch the dangling reference.
        let mut registry = valid_registry();
        registry.inject_for_test(
            None,
            None,
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/ghost/"),
                ),
                RegistrationId::from_raw(99),
            )),
        );
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("missing registration"), "{err}");
    }

    #[test]
    fn detects_storage_key_mismatch_in_index() {
        // Index key disagrees with the record's storage key.
        let mut registry = valid_registry();
        let mut broken = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        broken.storage_key = crate::key::StorageKey::from_raw("https://other.example");
        registry.inject_for_test(Some(broken), None, None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("storage key disagrees"), "{err}");
    }

    #[test]
    fn detects_registration_index_mismatch() {
        // Index key disagrees with the record's scope.
        let mut registry = valid_registry();
        let mut broken = registry
            .registration(RegistrationId::from_raw(1))
            .unwrap()
            .clone();
        broken.scope = Url::parse("https://example.com/other/").unwrap();
        registry.inject_for_test(Some(broken), None, None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("disagrees with its scope index key"), "{err}");

        // Index entry points at a missing registration.
        let mut registry = valid_registry();
        registry.inject_for_test(
            None,
            None,
            Some((
                (
                    crate::key::StorageKey::from_raw("https://example.com"),
                    String::from("https://example.com/ghost/"),
                ),
                RegistrationId::from_raw(99),
            )),
        );
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("missing registration"), "{err}");
    }

    #[test]
    fn detects_worker_registration_mismatch() {
        let mut registry = valid_registry();
        let mut orphan = worker(11, 99);
        orphan.registration = RegistrationId::from_raw(99);
        registry.inject_for_test(None, Some(orphan), None);
        let err = registry.check_invariants().unwrap_err();
        assert!(err.contains("missing registration"), "{err}");
    }
}
