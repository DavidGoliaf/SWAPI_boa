//! Storage key and trustworthiness check (TS §6.1, `R13.1`).

use url::{Host, Url};

use crate::error::{SwError, SwResult};

/// Storage isolation key (TS §6.1). Normally the serialized origin of the client.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct StorageKey(String);

impl StorageKey {
    /// Serializes `url`'s origin. Opaque origins (`data:`, `blob:` without an inner origin,
    /// `about:`) are rejected with [`SwError::InvalidUrl`].
    pub fn from_origin(url: &Url) -> SwResult<Self> {
        let origin = url.origin();
        match origin {
            url::Origin::Opaque(_) => Err(SwError::InvalidUrl(format!(
                "opaque origin: {}",
                url.as_str()
            ))),
            url::Origin::Tuple(_, _, _) => Ok(Self(origin.ascii_serialization())),
        }
    }

    /// Wraps a host-supplied key verbatim; no validation, no normalization.
    pub fn from_raw(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// Returns the storage key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for StorageKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `url`'s origin is potentially trustworthy (TS `R13.1`, W3C Secure Contexts).
///
/// Steps:
/// 1. scheme is `https` or `wss` → true
/// 2. host is an IPv4 address in `127.0.0.0/8`, or the IPv6 address `::1` → true
/// 3. host is `localhost` or ends with `.localhost` (ASCII case-insensitive) → true
/// 4. scheme is `file` → true
/// 5. otherwise → false
#[must_use]
pub fn is_potentially_trustworthy(url: &Url) -> bool {
    // Step 1: https or wss
    if url.scheme() == "https" || url.scheme() == "wss" {
        return true;
    }

    // Step 4: file
    if url.scheme() == "file" {
        return true;
    }

    // Steps 2-3 require a host
    match url.host() {
        Some(Host::Ipv4(ip)) => {
            // Step 2: 127.0.0.0/8
            let octets = ip.octets();
            octets[0] == 127
        }
        Some(Host::Ipv6(ip)) => {
            // Step 2: ::1
            ip == std::net::Ipv6Addr::LOCALHOST
        }
        Some(Host::Domain(domain)) => {
            // Step 3: localhost or *.localhost (ASCII case-insensitive)
            let lower = domain.to_ascii_lowercase();
            lower == "localhost" || lower.ends_with(".localhost")
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_origin_serializes() {
        // https://example.com/a/b?x → "https://example.com"
        let url = Url::parse("https://example.com/a/b?x").unwrap();
        let key = StorageKey::from_origin(&url).unwrap();
        assert_eq!(key.as_str(), "https://example.com");

        // non-default port preserved
        let url = Url::parse("https://example.com:8443").unwrap();
        let key = StorageKey::from_origin(&url).unwrap();
        assert_eq!(key.as_str(), "https://example.com:8443");

        // http://EXAMPLE.com → "http://example.com"
        let url = Url::parse("http://EXAMPLE.com").unwrap();
        let key = StorageKey::from_origin(&url).unwrap();
        assert_eq!(key.as_str(), "http://example.com");
    }

    #[test]
    fn from_origin_rejects_opaque() {
        // data:text/plain,x
        let url = Url::parse("data:text/plain,x").unwrap();
        assert!(StorageKey::from_origin(&url).is_err());

        // about:blank
        let url = Url::parse("about:blank").unwrap();
        assert!(StorageKey::from_origin(&url).is_err());
    }

    #[test]
    fn trustworthy_table() {
        // true cases
        assert!(is_potentially_trustworthy(
            &Url::parse("https://x").unwrap()
        ));
        assert!(is_potentially_trustworthy(&Url::parse("wss://x").unwrap()));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://localhost").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://localhost:8080").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://sub.localhost").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://LOCALHOST").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://127.0.0.1").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://127.5.5.5").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("http://[::1]").unwrap()
        ));
        assert!(is_potentially_trustworthy(
            &Url::parse("file:///tmp/a").unwrap()
        ));

        // false cases
        assert!(!is_potentially_trustworthy(
            &Url::parse("http://example.com").unwrap()
        ));
        assert!(!is_potentially_trustworthy(
            &Url::parse("ws://example.com").unwrap()
        ));
        assert!(!is_potentially_trustworthy(
            &Url::parse("http://128.0.0.1").unwrap()
        ));
        assert!(!is_potentially_trustworthy(
            &Url::parse("http://[::2]").unwrap()
        ));
        assert!(!is_potentially_trustworthy(
            &Url::parse("http://notlocalhost").unwrap()
        ));
        assert!(!is_potentially_trustworthy(
            &Url::parse("http://localhost.example.com").unwrap()
        ));
    }

    #[test]
    fn from_raw_is_verbatim() {
        let key = StorageKey::from_raw("weird key");
        assert_eq!(key.as_str(), "weird key");
    }
}
