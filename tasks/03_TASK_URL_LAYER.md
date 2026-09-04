# WORK ORDER: T-03 — URL layer: parsing, scope matching, path restriction

| Metadata | Value |
|---|---|
| **Task id** | `T-03-URL-LAYER` |
| **Milestone** | M0 |
| **Target crates** | `crates/boa_sw_core` only |
| **Normative TS** | `TZ_boa_sw_ServiceWorkers.md` §6.2 (URL handling and scope matching), `R6.4.1` (script MIME list), §2.1 |
| **Prerequisites** | `T-02` **accepted** (`error.rs`, `key.rs` exist and are stable) |
| **Diff budget** | ~500 lines (stop and escalate above ~750) |
| **Architect's role** | Every public item is specified below with its exact signature and behaviour. |
| **Implementer's role** | Implement exactly these items and their tests. **No other types, no registry, no job logic.** |

---

## 1. Guardrails

1. **`AD-2` — engine-free core.** Nothing in this task may reference `boa_engine`, `boa_gc`,
   `boa_runtime` or `boa_wintertc`. `T-01`'s CI job `dependency-direction` enforces it; do not
   weaken it.
2. **`R2.1.2`** — no `unwrap`/`expect`/`panic!`/`todo!` in library code outside tests.
   Documented-infallible cases carry a comment naming the invariant.
3. **`R2.1.3`** — no blocking I/O, no thread spawning, no allocation-heavy helpers on hot paths.
   `scope_matches` in particular is called once per candidate registration on every navigation and
   fetch dispatch (later tasks) — it MUST NOT allocate. Compare against the URLs' own serialized
   representations (`Url::as_str()` and friends); do not build new `String`s to compare.
4. **Reuse T-02, do not redefine.** `SwError::InvalidUrl`, `SwError::PathRestriction`,
   `SwError::BadScriptMime` already exist (`crates/boa_sw_core/src/error.rs`) with their
   `js_kind()`/`default_message()` mapping fixed by `AD-12`. This task produces `Result<_, SwError>`
   using those variants; it does not add new `SwError` variants or change the mapping.
5. **`R6.2.2`'s prefix behaviour is intentional, not a bug.** The Spec's scope-match algorithm
   compares *serialized strings*, so a scope of `/foo` matches a client URL of `/foobar`. Do not
   "fix" this by inserting a path-segment boundary check — it would diverge from the Spec and from
   real browsers. Document the surprise in the doc comment, keep the test that pins it.
6. Integration-test files (`crates/*/tests/*.rs`) start with
   `#![allow(clippy::unwrap_used, clippy::expect_used)]` (convention set in `T-01`).

---

## 2. Files to create / modify

```
crates/boa_sw_core/src/url_util.rs        # NEW
crates/boa_sw_core/src/lib.rs             # MODIFY: add `pub mod url_util;` + re-exports
docs/traceability.md                      # MODIFY: append this task's rows
docs/reviews/T-03-handoff.md              # NEW
```

Nothing else. Do not touch other crates, manifests, `deny.toml`, or CI. Do not touch `error.rs`,
`ids.rs`, `key.rs`, `clock.rs`, or `observe.rs` except to import from them.

---

## 3. Signatures and behaviour (normative — copy, do not redesign)

### 3.1. `src/url_util.rs`

```rust
use url::Url;
use crate::error::{SwError, SwResult};

/// Parses `input` against `base` (TS `R6.2.1`). Parse failure is `SwError::InvalidUrl`.
pub fn parse_with_base(input: &str, base: &Url) -> SwResult<Url>;

/// Serializes `u` without its fragment (`#...`), matching the Spec's "URL serializer, excluding
/// fragment" (used to build the client URL for scope matching, TS `R6.2.2`).
#[must_use]
pub fn serialize_exclude_fragment(u: &Url) -> String;

/// The default scope for a script at `script_url`: the script URL with its last path segment
/// removed, i.e. resolving `"./"` against it (TS `R6.2.6`).
#[must_use]
pub fn default_scope(script_url: &Url) -> Url;

/// Whether the *path* of `u` contains an encoded slash or backslash (`%2f`, `%2F`, `%5c`, `%5C`),
/// checked case-insensitively. Only the path is inspected — an encoded slash in the query or
/// fragment does not count (TS `R6.2.5`).
#[must_use]
pub fn has_encoded_slash(u: &Url) -> bool;

/// Whether `a` and `b` share the same origin (scheme, host, port), per `Url::origin()`.
#[must_use]
pub fn same_origin(a: &Url, b: &Url) -> bool;

/// **Scope match** (TS `R6.2.2`, `#scope-match-algorithm`): true iff the serialized `scope`
/// (fragment excluded) is a string prefix of the serialized `client_url` (fragment excluded).
///
/// MUST NOT allocate: compare the URLs' own string representations directly.
#[must_use]
pub fn scope_matches(scope: &Url, client_url: &Url) -> bool;

/// **Path restriction** (TS `R6.2.3`, `#path-restriction`): the scope's path MUST be prefixed by
/// `script_url`'s directory path, unless `allowed` is `Some(value)` and `value` parses (against
/// `script_url`) to a URL whose path is a prefix of the scope's path.
///
/// `allowed` is the already-extracted `Service-Worker-Allowed` header value, if any; this
/// function does not fetch or read headers.
///
/// # Errors
/// `SwError::PathRestriction` if neither condition holds.
pub fn path_restriction_ok(scope: &Url, script_url: &Url, allowed: Option<&str>) -> SwResult<()>;

/// Whether `essence` (an already-lowercased MIME essence, no parameters) is one of the JavaScript
/// MIME types accepted for a service worker script (TS `R6.4.1`).
///
/// The caller is responsible for extracting the essence from a full `Content-Type` value (e.g.
/// stripping `; charset=utf-8`) and lowercasing it; this function does not parse `Content-Type`.
#[must_use]
pub fn is_javascript_mime(essence: &str) -> bool;
```

**The accepted MIME essences (`R6.4.1`), copy exactly:**

```
application/javascript, application/ecmascript, application/x-ecmascript, application/x-javascript,
text/javascript, text/ecmascript, text/javascript1.0, text/javascript1.1, text/javascript1.2,
text/javascript1.3, text/javascript1.4, text/javascript1.5, text/jscript, text/livescript,
text/x-ecmascript, text/x-javascript
```

(`text/javascript1.0..1.5` in the TS table expands to the six explicit `text/javascript1.N`
essences above — there is no range syntax in a MIME essence.)

### 3.2. `src/lib.rs`

Append:

```rust
pub mod url_util;
```

No re-export of individual `url_util` functions at the crate root — later tasks (`T-04`+) call
them as `boa_sw_core::url_util::scope_matches(...)`, matching how the TS groups them. (This
mirrors `T-02`'s own module structure: functions are namespaced under their module, not
flattened into the crate root, except for the handful of types `T-02` explicitly re-exported.)

---

## 4. Tests to write

Unit tests live in `url_util.rs`'s `#[cfg(test)] mod tests`. No integration test file is needed
for this task (nothing new is re-exported at the crate root).

| Test | Asserts |
|---|---|
| `tests::parse_with_base_resolves_relative` | `parse_with_base("/a/b", &base)` against `https://example.com/x/` resolves to `https://example.com/a/b`; an absolute `input` ignores `base`. |
| `tests::parse_with_base_rejects_invalid` | `parse_with_base("http://[", &base)` → `Err(SwError::InvalidUrl(_))`. |
| `tests::serialize_exclude_fragment_drops_fragment` | `https://example.com/a#frag` → `"https://example.com/a"`; a URL with no fragment is unchanged. |
| `tests::default_scope_removes_last_segment` | `https://example.com/js/sw.js` → `https://example.com/js/`; `https://example.com/sw.js` → `https://example.com/`; a URL whose path is already `/` is unchanged. |
| `tests::has_encoded_slash_table` | ≥ 8 cases: `%2f`, `%2F`, `%5c`, `%5C` in the path → true; the same sequences in the query or fragment only → false; a plain path with a literal `/` → false. |
| `tests::same_origin_table` | same scheme+host+port → true; different scheme, different host, different port → false (one case each); `http://a` vs `https://a` → false. |
| `tests::scope_matches_table` | ≥ 14 cases, at least: `/` vs `/x` (true), `/foo` vs `/foobar` (true — the intentional prefix case, `R6.2.2`), `/foo/` vs `/foo` (false), scope with a query string vs a client URL differing only in fragment (true — fragment excluded from comparison), scope and client URL that differ only in fragment (true), different origins with identical paths (false), scope longer than client URL (false), exact equality (true). |
| `tests::path_restriction_ok_table` | ≥ 14 cases, at least: script `/js/sw.js` with scope `/` and no `allowed` → `Err`; same with `Service-Worker-Allowed: /` → `Ok`; `Service-Worker-Allowed: /a` with scope `/b` → `Err`; `Service-Worker-Allowed` given as a relative value, resolved against `script_url` → `Ok` when the resolved path is a prefix of scope; script and scope in the same directory with no header → `Ok`; malformed `allowed` value (fails to parse against `script_url`) → `Err`. |
| `tests::scope_matches_agrees_with_default_scope` (proptest) | For any script URL `s` generated from a small alphabet of paths, `scope_matches(&default_scope(&s), &s)` is `true`. Uses the crate's existing `proptest` dev-dependency; keep the input strategy narrow (fixed scheme/host, generated path segments) so the test is fast and deterministic in CI. |
| `tests::is_javascript_mime_table` | Every essence in the §3.1 list → `true`; `text/plain`, `application/json`, `text/javascript; charset=utf-8` (unstripped, so it must be `false` — the function does not parse parameters) → `false`. |
| `tests::scope_matches_does_not_allocate` | Documents the no-allocation requirement: either a `#[test]` using a counting/no-op global allocator guard already established by `T-01` if one exists, or — if none exists — a comment-only justification is **not** acceptable; write the test using `std::hint::black_box` around repeated calls and assert by code inspection in the doc comment that no `String`/`Vec`/`format!` appears in `scope_matches`'s body. Prefer the strongest check the workspace's existing tooling supports; note in the handoff which form was used. |

**Test-writing rules for this task:** one `proptest` test is expected here (unlike `T-02`, which
had none) — keep its strategy small and the case count at the `proptest` default (256) or lower so
CI stays fast. No snapshot tests.

---

## 5. Acceptance criteria

Run from the workspace root.

| # | Criterion | Command / check |
|---|---|---|
| 1 | Everything builds, all feature combinations | `cargo build -p boa_sw_core --all-features`, `… --no-default-features`, `… --features test-util` |
| 2 | Tests pass | `cargo test -p boa_sw_core --all-features` — every test of §4 present and green |
| 3 | wasm build unaffected | `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` |
| 4 | ≥ 30 combined table cases | `scope_matches_table` + `path_restriction_ok_table` together have at least 30 asserted cases; state the count in the handoff |
| 5 | No hot-path allocation in `scope_matches` | code inspection: no `String::new`, `.to_owned()`, `.to_string()`, `format!`, `Vec::new`, or collection literal inside the function body; quote the function in the handoff |
| 6 | Core is still engine-free (`R2.2.2`) | `cargo tree -p boa_sw_core --all-features --edges normal,build,dev \| grep -E 'boa_(engine\|gc\|runtime\|wintertc\|macros)'` → **no output** |
| 7 | Lints | `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` |
| 8 | Formatting | `cargo fmt --all --check` |
| 9 | No forbidden constructs in library code | `grep -nE '\.unwrap\(\)\|\.expect\(\|panic!\|todo!\|unimplemented!' crates/boa_sw_core/src/url_util.rs` → only lines inside `#[cfg(test)]` blocks |
| 10 | Module coverage ≥ 95 % (`R15.2.1`) | `cargo llvm-cov -p boa_sw_core --all-features --summary-only` if available; otherwise the per-function argument, as `T-02` did |
| 11 | `R6.2.2`'s prefix behaviour is pinned | `scope_matches_table` contains the `/foo` vs `/foobar` → `true` case; quote it in the handoff |
| 12 | Traceability updated | `docs/traceability.md` gains rows for `R6.2.1`–`R6.2.6`, `R6.4.1`, each naming a test from §4 |

---

## 6. Do not do

- No `Registry`, `RegistrationRecord`, `WorkerRecord`, `ScriptResource`, `Job`, or storage
  types — that is `T-04`/`T-05`.
- No HTTP fetching, no reading of `Service-Worker-Allowed` from an actual response — `allowed` is
  received as an already-extracted `Option<&str>`.
- `R6.2.4` (scope/script same-origin-with-client, scheme restricted to `http`/`https`) is **not**
  implemented as a combined check in this task. `same_origin` is provided as a general helper;
  the *calling* algorithm that enforces `R6.2.4` during registration belongs to `T-05` (Register
  algorithm). Do not add a `register_precondition_ok`-style function here.
- No changes to `Cargo.toml`, `deny.toml`, `ci.yml`, or any other crate.
- No new `SwError` variants and no change to the `js_kind()`/`default_message()` mapping fixed in
  `T-02`.

---

## 7. Handoff

Write `docs/reviews/T-03-handoff.md` with:

1. File-by-file summary of what was added.
2. All 12 acceptance criteria with the command and its result (quote grep output, including "no
   output" where that is the expected result).
3. Coverage numbers for `url_util.rs`, or the per-function coverage argument if `cargo llvm-cov`
   was unavailable.
4. Deviations — expected: none. Anything else must also appear in `docs/QUESTIONS.md`.
5. `docs/DECISIONS.md` entries added (expected: none).
6. Requirement ids covered, mirrored into `docs/traceability.md`.
7. Open questions for `T-04` (records, registry, storage traits), which builds directly on
   `url_util.rs` (`Registry::match_registration` calls `scope_matches`).

Then stop. `T-04` is the next work order.
