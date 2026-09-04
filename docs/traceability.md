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

*(every later task appends its rows)*

## 2. Specification algorithms → modules

Copy of TS Appendix E, extended with a "covering tests" column as tasks land.
