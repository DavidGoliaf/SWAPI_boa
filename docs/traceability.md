# Traceability

Two tables, both required by TS §15.2.2 and checked at release by `scripts/check_traceability.rs`
(written in `T-26`).

## 1. Requirements → tests

| Requirement | Where implemented | Covering tests | Task |
|---|---|---|---|
| `R2.1.1` | workspace lints, CI | `ci: fmt`, `ci: clippy`, `ci: deny` | T-01 |
| `R2.2.1` | crate manifests | `ci: dependency-direction` | T-01 |
| `R2.2.2` | `crates/boa_sw_core/Cargo.toml` | `ci: dependency-direction` | T-01 |
| `R2.4.1` | crate features | `ci: feature matrix` | T-01 |
| `R6.1.1` | `crates/boa_sw_core/src/ids.rs` | `ids::tests::allocate_starts_at_one`, `ids::tests::restore_from_continues_above_max`, `ids::tests::default_matches_new` | T-02 |
| `R6.1.2` | `crates/boa_sw_core/src/ids.rs` | `ids::tests::display_format` | T-02 |
| `R13.1` | `crates/boa_sw_core/src/key.rs` | `key::tests::trustworthy_table` | T-02 |
| `R12.1` | `crates/boa_sw_core/src/error.rs` | `error::tests::js_kind_is_total`, `error::tests::default_message_table` | T-02 |
| `R4.2.9` | `crates/boa_sw_core/src/observe.rs` | `observe::tests::null_observer_is_inert` | T-02 |
| `R4.2.11` | `crates/boa_sw_core/src/observe.rs` | `observe::tests::null_observer_is_inert` | T-02 |
| `R6.2.1` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::parse_with_base_resolves_relative`, `url_util::tests::parse_with_base_rejects_invalid` | T-03 |
| `R6.2.2` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::scope_matches_table`, `url_util::tests::scope_matches_does_not_allocate` | T-03 |
| `R6.2.3` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::path_restriction_ok_table` | T-03 |
| `R6.2.5` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::has_encoded_slash_table` | T-03 |
| `R6.2.6` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::default_scope_removes_last_segment`, `url_util::tests::scope_matches_agrees_with_default_scope` | T-03 |
| `R6.4.1` | `crates/boa_sw_core/src/url_util.rs` | `url_util::tests::is_javascript_mime_table` | T-03 |
| `R4.2.6` | `crates/boa_sw_core/src/storage.rs` | `storage::tests::trait_signature_smoke` | T-04 |
| `R4.2.7` | `crates/boa_sw_core/src/storage.rs` | `storage::tests::trait_signature_smoke` | T-04 |
| `R4.2.8` | `crates/boa_sw_core/src/storage.rs` | `storage::tests::trait_signature_smoke` | T-04 |
| `R6.1.3` | `crates/boa_sw_core/src/registry.rs` | `registry::tests::scope_with_fragment_is_normalized_on_insert`, `registry::tests::get_registration_isolated_by_key` | T-04 |
| `R6.3.1` | `crates/boa_sw_core/src/model.rs` | `model::tests::serde_round_trip_records` | T-04 |
| `R6.3.2` | `crates/boa_sw_core/src/registry.rs` | `registry::tests::replace_methods_preserve_indexes` | T-04 |
| `R6.4.5` | `crates/boa_sw_core/src/model.rs` | `model::tests::script_resource_map_preserves_absolute_url_keys` | T-04 |
| `R6.5.1` | `crates/boa_sw_core/src/registry.rs` | `registry::tests::newest_worker_precedence` | T-04 |
| `R6.5.2` | `crates/boa_sw_core/src/registry.rs` | `registry::tests::match_registration_skips_uninstalling` | T-04 |
| `R15.5.3` | `crates/boa_sw_core/src/invariants.rs` | `invariants::tests::detects_worker_in_two_slots`, `invariants::tests::detects_empty_non_uninstalling_registration`, `invariants::tests::detects_running_redundant_worker`, `invariants::tests::detects_duplicate_or_dangling_ids`, `invariants::tests::detects_registration_index_mismatch`, `invariants::tests::detects_worker_registration_mismatch` | T-04 |

*(every later task appends its rows)*

## 2. Specification algorithms → modules

Copy of TS Appendix E, extended with a "covering tests" column as tasks land.
