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

*(every later task appends its rows)*

## 2. Specification algorithms → modules

Copy of TS Appendix E, extended with a "covering tests" column as tasks land.
