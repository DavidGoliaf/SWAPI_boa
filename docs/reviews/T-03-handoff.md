# T-03 Handoff — URL layer: parsing, scope matching, path restriction

## Summary

Implemented the URL utility module for `boa_sw_core` as specified in TS §6.2 and `R6.4.1`, per
`tasks/03_TASK_URL_LAYER.md`.

## Files created/modified

### New files
- `crates/boa_sw_core/src/url_util.rs` — `parse_with_base`, `serialize_exclude_fragment`,
  `default_scope`, `has_encoded_slash`, `same_origin`, `scope_matches`, `path_restriction_ok`,
  `is_javascript_mime`, and the private `strip_fragment` helper.

### Modified files
- `crates/boa_sw_core/src/lib.rs` — added `pub mod url_util;` (no crate-root re-exports, per the
  work order: later tasks call these as `boa_sw_core::url_util::...`).
- `docs/traceability.md` — added requirement rows for T-03.

## Acceptance criteria

All commands run with `CARGO_TARGET_DIR=/tmp/boa-sw-target` (local disk) from the workspace root,
to avoid `vboxsf` shared-folder I/O slowness (same workaround as `T-01`/`T-02`).

| # | Criterion | Command | Result |
|---|---|---|---|
| 1 | Builds, all feature combinations | `cargo build -p boa_sw_core --all-features`, `--no-default-features`, `--features test-util` | ✅ all `Finished` |
| 2 | Tests pass | `cargo test -p boa_sw_core --all-features` | ✅ 32 unit + 1 integration test passing |
| 3 | wasm build unaffected | `cargo build -p boa_sw_core --target wasm32-unknown-unknown --no-default-features` | ✅ |
| 4 | ≥ 30 combined table cases | `scope_matches_table` (17 cases) + `path_restriction_ok_table` (15 `assert!`s) | ✅ 32 total |
| 5 | No hot-path allocation in `scope_matches` | code inspection (quoted below) | ✅ |
| 6 | Core is still engine-free | `cargo tree -p boa_sw_core --all-features --edges normal,build,dev \| grep -E 'boa_(engine\|gc\|runtime\|wintertc\|macros)'` | no output |
| 7 | Lints | `cargo clippy -p boa_sw_core --all-targets --all-features -- -D warnings` | ✅ clean |
| 8 | Formatting | `cargo fmt --all --check` | ✅ clean |
| 9 | No forbidden constructs in library code | `grep -nE '\.unwrap\(\)\|\.expect\(\|panic!\|todo!\|unimplemented!' crates/boa_sw_core/src/url_util.rs` | only inside `#[cfg(test)]` (the `url()` test helper and its callers) |
| 10 | Module coverage ≥ 95 % | per-function argument (see below); `cargo llvm-cov` not installed (offline environment) | ✅ |
| 11 | `R6.2.2` prefix behaviour pinned | `scope_matches_table` case `("https://example.com/foo", "https://example.com/foobar", true)`, annotated "Intentional prefix behaviour, R6.2.2" | ✅ present |
| 12 | Traceability updated | `docs/traceability.md` | ✅ rows added for R6.2.1, R6.2.2, R6.2.3, R6.2.5, R6.2.6, R6.4.1 |

### Criterion 5 — no-allocation body of `scope_matches`

```rust
pub fn scope_matches(scope: &Url, client_url: &Url) -> bool {
    let scope_str = strip_fragment(scope.as_str());
    let client_str = strip_fragment(client_url.as_str());
    client_str.starts_with(scope_str)
}

fn strip_fragment(s: &str) -> &str {
    match s.find('#') {
        Some(idx) => &s[..idx],
        None => s,
    }
}
```

Both functions operate on `&str` borrows (`Url::as_str()`, slice indexing); neither constructs a
`String`, `Vec`, or uses `format!`. `scope_matches_does_not_allocate` exercises the function under
`std::hint::black_box` (no allocator-counting harness exists in this workspace to assert this
programmatically — the same limitation `T-02` would have hit for a similar claim).

### Criterion 10 — per-function coverage argument

- `parse_with_base` — `parse_with_base_resolves_relative` (relative + absolute input),
  `parse_with_base_rejects_invalid`, plus indirectly via `path_restriction_ok_table`'s relative
  and malformed `allowed` cases.
- `serialize_exclude_fragment` — `serialize_exclude_fragment_drops_fragment` (with and without a
  fragment).
- `default_scope` — `default_scope_removes_last_segment` (3 cases) and the property test
  `scope_matches_agrees_with_default_scope`.
- `has_encoded_slash` — `has_encoded_slash_table` (10 cases: both encodings, both cases,
  path vs. query/fragment, fragment-only encoded slash, plain path).
- `same_origin` — `same_origin_table` (4 cases).
- `scope_matches` (and `strip_fragment`) — `scope_matches_table` (17 cases, incl. two scope-with-`#frag` cases) and
  `scope_matches_agrees_with_default_scope`.
- `path_restriction_ok` — `path_restriction_ok_table` (15 cases, both branches and the
  malformed-`allowed` fallthrough).
- `is_javascript_mime` — `is_javascript_mime_table` (all 16 accepted essences plus 3 rejected).

## Deviations

One genuine work-order-vs-TS discrepancy, found in review and fixed on this branch (escalated as
`docs/QUESTIONS.md / Q-02`, answer pending):

- **`scope_matches` fragment handling.** Work order §3.1 excludes the fragment from *both* sides;
  TS `R6.2.2` excludes it from the client URL only. The implementation now follows the work order
  (strips both). On valid inputs the two readings agree — stored scopes never carry a fragment
  (`R6.1.3`) — so `T-04`'s `match_registration` is unaffected either way; the difference is only
  observable for direct calls with an un-normalized scope, pinned by two new
  `scope_matches_table` cases (`scope` with `#frag`).

One interpretive note, not a deviation from anything explicit in the work order (recorded here
since it affects `path_restriction_ok`'s observable behaviour on malformed input, which the work
order left implicit; escalated as `docs/QUESTIONS.md / Q-03`):

- **Malformed `Service-Worker-Allowed` value.** `R6.2.3` does not say what happens when the header
  value fails to parse against the script URL. `path_restriction_ok` treats a malformed `allowed`
  the same as `None` (falls through to the default script-directory check) rather than
  propagating `SwError::InvalidUrl`, so the function's error type stays uniformly
  `SwError::PathRestriction`. This matches how a browser would behave (a malformed response header
  doesn't surface a URL-parse error to the caller) and keeps the function's contract simple. Tests:
  `path_restriction_ok_table`'s two `Some("http://[")` cases (one falling through to `Err`, one to
  `Ok` because the default check already passes).

No new dependencies added.

## Decisions

No new `DECISIONS.md` entries required. All choices specified in the TS or the work order, except
the malformed-`allowed` handling noted above, which did not rise to the level of an `I-*` decision
(no alternative was seriously in tension with `AD-12` or any other fixed decision).

## Open questions for T-04

None blocking. One thing for `T-04`'s author to note: `Registry::match_registration` (§6.5) will
call `url_util::scope_matches` once per candidate registration to find the longest match — the
no-allocation guarantee here is what makes that loop cheap; do not wrap `scope_matches`'s inputs
in anything that reintroduces an allocation (e.g. re-serializing a stored `Url` through
`to_string()` before comparing).
