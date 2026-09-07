//! URL handling: parsing, scope matching, path restriction (TS §6.2, `R6.4.1`).

use url::Url;

use crate::error::{SwError, SwResult};

/// The JavaScript MIME essences accepted for a service worker script (TS `R6.4.1`).
const JAVASCRIPT_MIME_ESSENCES: &[&str] = &[
    "application/javascript",
    "application/ecmascript",
    "application/x-ecmascript",
    "application/x-javascript",
    "text/javascript",
    "text/ecmascript",
    "text/javascript1.0",
    "text/javascript1.1",
    "text/javascript1.2",
    "text/javascript1.3",
    "text/javascript1.4",
    "text/javascript1.5",
    "text/jscript",
    "text/livescript",
    "text/x-ecmascript",
    "text/x-javascript",
];

/// Parses `input` against `base` (TS `R6.2.1`). Parse failure is [`SwError::InvalidUrl`].
///
/// An absolute `input` (one carrying its own scheme) ignores `base`, per the URL parser.
pub fn parse_with_base(input: &str, base: &Url) -> SwResult<Url> {
    base.join(input)
        .map_err(|_| SwError::InvalidUrl(input.to_owned()))
}

/// Serializes `u` without its fragment (`#...`), matching the Spec's "URL serializer, excluding
/// fragment".
#[must_use]
pub fn serialize_exclude_fragment(u: &Url) -> String {
    let mut u = u.clone();
    u.set_fragment(None);
    u.to_string()
}

/// The default scope for a script at `script_url`: the script URL with its last path segment
/// removed, i.e. the resolution of `"./"` against it (TS `R6.2.6`).
#[must_use]
pub fn default_scope(script_url: &Url) -> Url {
    // `join` only fails for a base that cannot-be-a-base (e.g. a `data:` URL). Callers pass an
    // http(s) script URL, which is always a base URL, so this never fails in practice.
    script_url.join("./").unwrap_or_else(|_| script_url.clone())
}

/// Whether the *path* of `u` contains an encoded slash or backslash (`%2f`, `%5c`, any case),
/// checked case-insensitively (TS `R6.2.5`). The query and fragment are not inspected.
#[must_use]
pub fn has_encoded_slash(u: &Url) -> bool {
    u.path()
        .as_bytes()
        .windows(3)
        .any(|w| w.eq_ignore_ascii_case(b"%2f") || w.eq_ignore_ascii_case(b"%5c"))
}

/// Whether `a` and `b` share the same origin (scheme, host, port).
#[must_use]
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.origin() == b.origin()
}

/// Strips a trailing `#fragment` from a serialized URL string without allocating.
fn strip_fragment(s: &str) -> &str {
    match s.find('#') {
        Some(idx) => &s[..idx],
        None => s,
    }
}

/// **Scope match** (TS `R6.2.2`, `#scope-match-algorithm`): true iff the serialized `scope`
/// (fragment excluded) is a string prefix of the serialized `client_url` (fragment excluded).
///
/// `/foo` matching `/foobar` is intentional (a known Spec property, not a bug) — the comparison
/// is over serialized strings, not path segments.
///
/// The fragment is excluded from *both* sides (work order `T-03` §3.1). This agrees with the TS
/// on every valid input: stored scopes never carry a fragment (`R6.1.3`), so stripping the
/// scope's fragment is a no-op in practice; it only makes direct calls with an
/// un-normalized scope behave the same. See `Q-02` in `docs/QUESTIONS.md`.
///
/// MUST NOT allocate: compares the URLs' own string representations directly.
#[must_use]
pub fn scope_matches(scope: &Url, client_url: &Url) -> bool {
    let scope_str = strip_fragment(scope.as_str());
    let client_str = strip_fragment(client_url.as_str());
    client_str.starts_with(scope_str)
}

/// **Path restriction** (TS `R6.2.3`, `#path-restriction`): the scope's path MUST be prefixed by
/// `script_url`'s directory path, unless `allowed` is `Some(value)` and `value` parses (against
/// `script_url`) to a URL whose path is a prefix of the scope's path.
///
/// `allowed` is the already-extracted `Service-Worker-Allowed` header value, if any; this
/// function does not fetch or read headers. A malformed `allowed` value is treated the same as
/// its absence (the override does not apply), not as a parse error.
///
/// # Errors
/// [`SwError::PathRestriction`] if neither condition holds.
pub fn path_restriction_ok(scope: &Url, script_url: &Url, allowed: Option<&str>) -> SwResult<()> {
    let script_dir = default_scope(script_url);
    let under_script_dir = scope.path().starts_with(script_dir.path());
    let under_allowed = allowed
        .and_then(|value| parse_with_base(value, script_url).ok())
        .is_some_and(|allowed_url| scope.path().starts_with(allowed_url.path()));

    if under_script_dir || under_allowed {
        Ok(())
    } else {
        Err(SwError::PathRestriction)
    }
}

/// Whether `essence` (an already-lowercased MIME essence, no parameters) is one of the JavaScript
/// MIME types accepted for a service worker script (TS `R6.4.1`).
///
/// The caller is responsible for extracting the essence from a full `Content-Type` value (e.g.
/// stripping `; charset=utf-8`) and lowercasing it; this function does not parse `Content-Type`.
#[must_use]
pub fn is_javascript_mime(essence: &str) -> bool {
    JAVASCRIPT_MIME_ESSENCES.contains(&essence)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn parse_with_base_resolves_relative() {
        let base = url("https://example.com/x/");
        assert_eq!(
            parse_with_base("/a/b", &base).unwrap().as_str(),
            "https://example.com/a/b"
        );
        assert_eq!(
            parse_with_base("c", &base).unwrap().as_str(),
            "https://example.com/x/c"
        );
        // Absolute input ignores base.
        assert_eq!(
            parse_with_base("https://other.example/", &base)
                .unwrap()
                .as_str(),
            "https://other.example/"
        );
    }

    #[test]
    fn parse_with_base_rejects_invalid() {
        let base = url("https://example.com/");
        assert!(matches!(
            parse_with_base("http://[", &base),
            Err(SwError::InvalidUrl(_))
        ));
    }

    #[test]
    fn serialize_exclude_fragment_drops_fragment() {
        assert_eq!(
            serialize_exclude_fragment(&url("https://example.com/a#frag")),
            "https://example.com/a"
        );
        assert_eq!(
            serialize_exclude_fragment(&url("https://example.com/a")),
            "https://example.com/a"
        );
    }

    #[test]
    fn default_scope_removes_last_segment() {
        assert_eq!(
            default_scope(&url("https://example.com/js/sw.js")).as_str(),
            "https://example.com/js/"
        );
        assert_eq!(
            default_scope(&url("https://example.com/sw.js")).as_str(),
            "https://example.com/"
        );
        assert_eq!(
            default_scope(&url("https://example.com/")).as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn has_encoded_slash_table() {
        // In the path -> true.
        assert!(has_encoded_slash(&url("https://example.com/a%2fb")));
        assert!(has_encoded_slash(&url("https://example.com/a%2Fb")));
        assert!(has_encoded_slash(&url("https://example.com/a%5cb")));
        assert!(has_encoded_slash(&url("https://example.com/a%5Cb")));
        assert!(has_encoded_slash(&url("https://example.com/%2f")));
        // In the query or fragment only -> false.
        assert!(!has_encoded_slash(&url("https://example.com/a?x=%2f#y%5c")));
        // Fragment-only encoded slash (clean query) -> false.
        assert!(!has_encoded_slash(&url("https://example.com/a#frag%2f")));
        // Plain path, no encoded slash -> false.
        assert!(!has_encoded_slash(&url("https://example.com/a/b")));
        assert!(!has_encoded_slash(&url("https://example.com/")));
    }

    #[test]
    fn same_origin_table() {
        assert!(same_origin(
            &url("https://a.example/x"),
            &url("https://a.example/y")
        ));
        assert!(!same_origin(
            &url("https://a.example/"),
            &url("https://b.example/")
        ));
        assert!(!same_origin(
            &url("https://a.example/"),
            &url("http://a.example/")
        ));
        assert!(!same_origin(
            &url("https://a.example:1234/"),
            &url("https://a.example:5678/")
        ));
    }

    #[test]
    fn scope_matches_table() {
        let cases: &[(&str, &str, bool)] = &[
            ("https://example.com/", "https://example.com/x", true),
            // Intentional prefix behaviour, R6.2.2: `/foo` matches `/foobar`.
            (
                "https://example.com/foo",
                "https://example.com/foobar",
                true,
            ),
            ("https://example.com/foo/", "https://example.com/foo", false),
            // Fragment excluded from the serialized comparison (both sides).
            (
                "https://example.com/foo?x=1",
                "https://example.com/foo?x=1#y",
                true,
            ),
            ("https://example.com/foo", "https://example.com/foo#b", true),
            // Scope carrying a fragment (never stored that way per R6.1.3, see Q-02):
            // the fragment is excluded from both sides before comparing.
            (
                "https://example.com/foo#frag",
                "https://example.com/foobar",
                true,
            ),
            (
                "https://example.com/foo#frag",
                "https://example.com/foo",
                true,
            ),
            // Different origin, identical path.
            ("https://a.example/foo", "https://b.example/foo", false),
            // Scope longer than the client URL.
            (
                "https://example.com/foobar",
                "https://example.com/foo",
                false,
            ),
            // Exact equality.
            ("https://example.com/foo", "https://example.com/foo", true),
            // Nested prefix.
            ("https://example.com/a/b", "https://example.com/a/b/c", true),
            // Trailing-slash boundary: scope ends in `/`, client does not extend past it.
            (
                "https://example.com/a/b/",
                "https://example.com/a/bc",
                false,
            ),
            // Different scheme.
            ("http://example.com/foo", "https://example.com/foo", false),
            // Different port.
            (
                "https://example.com:8443/foo",
                "https://example.com/foo",
                false,
            ),
            // Root scope matches any deep path on the same origin.
            ("https://example.com/", "https://example.com/a/b/c/d", true),
            // Query differs -> not a string prefix.
            (
                "https://example.com/foo?x=1",
                "https://example.com/foo?x=2",
                false,
            ),
            // Client shorter than scope even though it starts the same way.
            (
                "https://example.com/foo/bar",
                "https://example.com/foo",
                false,
            ),
        ];
        for (scope, client, expected) in cases {
            assert_eq!(
                scope_matches(&url(scope), &url(client)),
                *expected,
                "scope={scope} client={client}"
            );
        }
    }

    #[test]
    fn scope_matches_does_not_allocate() {
        // `scope_matches` computes only `&str` slices via `strip_fragment` and `str::starts_with`
        // (see its body above); it never constructs a `String`, `Vec`, or `format!` result. No
        // allocator-counting harness exists in this workspace, so this exercises the function
        // under `black_box` to keep the compiler from folding it away; the no-allocation claim is
        // verified by code inspection, documented on the function itself.
        let scope = url("https://example.com/foo/");
        let client = url("https://example.com/foo/bar?x=1#y");
        for _ in 0..1000 {
            let _ = std::hint::black_box(scope_matches(
                std::hint::black_box(&scope),
                std::hint::black_box(&client),
            ));
        }
    }

    #[test]
    fn path_restriction_ok_table() {
        let script = url("https://example.com/js/sw.js");

        // No header: scope must be under the script's directory.
        assert!(path_restriction_ok(&url("https://example.com/"), &script, None).is_err());
        assert!(path_restriction_ok(&url("https://example.com/js/"), &script, None).is_ok());
        assert!(path_restriction_ok(&url("https://example.com/js/sub/"), &script, None).is_ok());
        assert!(path_restriction_ok(&url("https://example.com/other/"), &script, None).is_err());

        // Service-Worker-Allowed widens the max scope.
        assert!(path_restriction_ok(&url("https://example.com/"), &script, Some("/")).is_ok());
        assert!(path_restriction_ok(&url("https://example.com/b/"), &script, Some("/a")).is_err());
        assert!(path_restriction_ok(&url("https://example.com/a/"), &script, Some("/a")).is_ok());
        // Exact-equality boundary.
        assert!(path_restriction_ok(&url("https://example.com/x"), &script, Some("/x")).is_ok());
        // Relative allowed value, resolved against the script URL.
        assert!(path_restriction_ok(&url("https://example.com/"), &script, Some("../")).is_ok());
        assert!(path_restriction_ok(&url("https://example.com/js/"), &script, Some("./")).is_ok());

        // Malformed allowed value: treated as absent, falls through to the default check.
        assert!(
            path_restriction_ok(&url("https://example.com/"), &script, Some("http://[")).is_err()
        );
        assert!(
            path_restriction_ok(&url("https://example.com/js/"), &script, Some("http://[")).is_ok()
        );

        // Root script: any scope on the origin is under its directory.
        let root_script = url("https://example.com/sw.js");
        assert!(
            path_restriction_ok(&url("https://example.com/anything/"), &root_script, None).is_ok()
        );

        // Allowed value narrower than the requested scope still fails.
        assert!(
            path_restriction_ok(&url("https://example.com/a/b/"), &script, Some("/a/bb")).is_err()
        );

        // Absolute allowed value on a different path than the script's own directory.
        assert!(
            path_restriction_ok(
                &url("https://example.com/other/"),
                &script,
                Some("https://example.com/other/")
            )
            .is_ok()
        );
    }

    #[test]
    fn is_javascript_mime_table() {
        for essence in JAVASCRIPT_MIME_ESSENCES {
            assert!(is_javascript_mime(essence), "{essence} should be accepted");
        }
        assert!(!is_javascript_mime("text/plain"));
        assert!(!is_javascript_mime("application/json"));
        // Essence extraction (stripping parameters) is the caller's job.
        assert!(!is_javascript_mime("text/javascript; charset=utf-8"));
    }

    proptest! {
        #[test]
        fn scope_matches_agrees_with_default_scope(
            dir in "[a-z]{1,6}",
            file in "[a-z]{1,6}",
        ) {
            let script = url(&format!("https://example.com/{dir}/{file}.js"));
            let scope = default_scope(&script);
            prop_assert!(scope_matches(&scope, &script));
        }
    }
}
