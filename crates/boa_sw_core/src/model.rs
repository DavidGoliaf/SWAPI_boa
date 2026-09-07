//! Record model: workers, registrations, script resources (TS §6.3, §6.4).
//!
//! Plain Rust data; no engine, no I/O, no lifecycle logic. State transitions belong to later
//! tasks (`R6.3.2`); this module only stores state.

use std::rc::Rc;

use indexmap::IndexMap;
use smallvec::SmallVec;
use url::Url;

use crate::ids::{RegistrationId, WorkerId};
use crate::key::StorageKey;

/// Lifecycle state of a worker (TS §6.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WorkerState {
    /// Parsed but not yet installing.
    Parsed,
    /// Currently installing.
    Installing,
    /// Installed, waiting to activate.
    Installed,
    /// Currently activating.
    Activating,
    /// Active, controlling clients.
    Activated,
    /// Discarded; never runs again.
    Redundant,
}

/// How a worker's script is evaluated (TS §6.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WorkerType {
    /// Classic script evaluated with `importScripts` support.
    Classic,
    /// ES module worker (feature `module-workers`).
    Module,
}

/// Which scripts bypass the HTTP cache on update (TS §6.4, `R6.4.4`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum UpdateViaCache {
    /// Main script bypasses the cache; imports use it (the default).
    #[default]
    Imports,
    /// All scripts use the HTTP cache.
    All,
    /// No script uses the HTTP cache.
    None,
}

/// Runtime status of a worker; not persisted (TS §6.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RunState {
    /// Not currently running; may be started on demand.
    NotRunning,
    /// Realm creation / evaluation in progress.
    Starting,
    /// Running with a live realm.
    Running,
    /// Termination in progress.
    Terminating,
}

/// Event types with at least one registered listener at end of initial evaluation (TS §6.3).
///
/// Closed set fixed by work order `T-04`; later tasks record these values but do not rename them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EventType {
    /// `install` event listener present.
    Install,
    /// `activate` event listener present.
    Activate,
    /// `fetch` event listener present.
    Fetch,
    /// `message` event listener present.
    Message,
    /// `push` event listener present.
    Push,
    /// `sync` event listener present.
    Sync,
    /// `notificationclick` event listener present.
    NotificationClick,
    /// `notificationclose` event listener present.
    NotificationClose,
}

/// A service worker instance record (TS §6.3).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WorkerRecord {
    /// Worker identity.
    pub id: WorkerId,
    /// Registration this worker belongs to.
    pub registration: RegistrationId,
    /// URL of the main script.
    pub script_url: Url,
    /// Classic or module worker.
    pub worker_type: WorkerType,
    /// Lifecycle state; changed only through `SwCore` state transitions (`R6.3.2`).
    pub state: WorkerState,
    /// Set by `skipWaiting()`.
    pub skip_waiting: bool,
    /// Set once the script has been evaluated at least once; used by `importScripts` (§11.4.2).
    pub imported_scripts_updated: bool,
    /// `None` until the script has been evaluated once; then whether a `fetch` listener exists.
    pub has_fetch_handler: Option<bool>,
    /// Event types with at least one registered listener at end of initial evaluation.
    #[cfg_attr(feature = "serde", serde(with = "serde_smallvec_event_types"))]
    pub handled_event_types: SmallVec<[EventType; 8]>,
    /// Runtime status; not persisted.
    pub run_state: RunState,
    /// Number of unsettled extended lifetime promises + in-flight dispatches (§11.3).
    pub pending_events: u32,
    /// Last activity timestamp from `Clock::now_ms`.
    pub last_activity_ms: u64,
}

/// A registration record: one `(storage key, scope)` entry owning up to three workers (TS §6.3).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RegistrationRecord {
    /// Registration identity.
    pub id: RegistrationId,
    /// Storage isolation key.
    pub storage_key: StorageKey,
    /// Scope URL, normalized without a fragment; query preserved (`R6.1.3`).
    pub scope: Url,
    /// Which scripts bypass the HTTP cache on update.
    pub update_via_cache: UpdateViaCache,
    /// Worker currently installing, if any.
    pub installing: Option<WorkerId>,
    /// Worker waiting to activate, if any.
    pub waiting: Option<WorkerId>,
    /// Currently active worker, if any.
    pub active: Option<WorkerId>,
    /// Last update-check timestamp from `Clock::now_ms`.
    pub last_update_check_ms: Option<u64>,
    /// Navigation preload enabled flag.
    pub navigation_preload_enabled: bool,
    /// Navigation preload header value; default `"true"`.
    pub navigation_preload_header: String,
    /// Set by Unregister while an active worker still controls clients.
    pub uninstalling: bool,
}

/// A fetched script resource (TS §6.4).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptResource {
    /// Absolute URL of the script.
    pub url: Url,
    /// Script bytes.
    #[cfg_attr(feature = "serde", serde(with = "serde_bytes_rc"))]
    pub bytes: Rc<[u8]>,
    /// SHA-256 of `bytes`; lengths must also match before any hash comparison (`R6.4.3`).
    pub sha256: [u8; 32],
    /// Already-lowercased MIME essence.
    pub mime: String,
    /// Raw `Service-Worker-Allowed` header value, if present.
    pub service_worker_allowed: Option<String>,
    /// Whether this is the main script (as opposed to an import).
    pub is_main: bool,
}

/// Script resource map of a worker, keyed by absolute URL; insertion order preserved (`R6.4.5`).
pub type ScriptResourceMap = IndexMap<Url, ScriptResource>;

/// `Rc<[u8]>` byte buffer as a plain byte buffer on the wire.
///
/// `serde`'s `Rc` impls require the `rc` feature, which this crate does not enable; serializing
/// the bytes directly keeps the `serde` feature dependency-free.
#[cfg(feature = "serde")]
mod serde_bytes_rc {
    use std::rc::Rc;

    pub fn serialize<S>(bytes: &Rc<[u8]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&bytes.as_ref(), serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Rc<[u8]>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        Ok(Rc::from(bytes.into_boxed_slice()))
    }
}

/// `SmallVec<[EventType; 8]>` as a plain `Vec<EventType>` on the wire.
///
/// `smallvec`'s own `serde` impls live behind its `serde` feature, which this crate does not
/// enable (the `serde` feature here must stay dependency-free per work order `T-04` §1.7).
#[cfg(feature = "serde")]
mod serde_smallvec_event_types {
    use smallvec::SmallVec;

    use super::EventType;

    pub fn serialize<S>(events: &SmallVec<[EventType; 8]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_seq(events.iter())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SmallVec<[EventType; 8]>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let events: Vec<EventType> = serde::Deserialize::deserialize(deserializer)?;
        Ok(SmallVec::from_vec(events))
    }
}

/// Test-only `serde` token harness shared by this module's and `storage`'s round-trip tests.
///
/// No format crate is involved: `Serialize` drives tokens into a `Vec<Token>` and `Deserialize`
/// reads them back, proving both impls agree. Length-prefixed sequences carry their element
/// count, so no end markers are needed.
#[cfg(all(test, feature = "serde"))]
pub(crate) mod serde_token {
    use std::fmt;

    use serde::de::{
        self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
    };
    use serde::ser::{self, Serialize};

    /// One serialized datum.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Token {
        /// `bool`.
        Bool(bool),
        /// `u8`.
        U8(u8),
        /// `u16`.
        U16(u16),
        /// `u32`.
        U32(u32),
        /// `u64`.
        U64(u64),
        /// `str`.
        Str(String),
        /// Byte buffer.
        Bytes(Vec<u8>),
        /// `None`.
        None,
        /// `Some` marker; the value follows.
        Some,
        /// Unit / unit struct.
        Unit,
        /// Sequence of exactly `n` elements.
        Seq(usize),
        /// Tuple of exactly `n` elements.
        Tuple(usize),
        /// Map of exactly `n` entries.
        Map(usize),
        /// Struct of exactly `n` fields.
        Struct(usize),
        /// Enum unit variant name.
        Variant(String),
    }

    /// Harness error.
    #[derive(Clone, Debug, PartialEq)]
    pub struct TokenError(String);

    impl std::error::Error for TokenError {}

    impl TokenError {
        fn custom<T: fmt::Display>(msg: T) -> Self {
            Self(msg.to_string())
        }
    }

    impl fmt::Display for TokenError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl ser::Error for TokenError {
        fn custom<T: fmt::Display>(msg: T) -> Self {
            Self::custom(msg)
        }
    }

    impl de::Error for TokenError {
        fn custom<T: fmt::Display>(msg: T) -> Self {
            Self::custom(msg)
        }
    }

    /// Drives `Serialize` into tokens.
    #[derive(Debug, Default)]
    pub struct TokenSerializer {
        tokens: Vec<Token>,
    }

    impl TokenSerializer {
        /// Borrowed tokens, for adapter tests that serialize and deserialize in two steps.
        #[cfg(test)]
        pub fn tokens_for_test(&self) -> Vec<Token> {
            self.tokens.clone()
        }
    }

    impl ser::Serializer for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;
        type SerializeSeq = Self;
        type SerializeTuple = Self;
        type SerializeTupleStruct = Self;
        type SerializeTupleVariant = Self;
        type SerializeMap = Self;
        type SerializeStruct = Self;
        type SerializeStructVariant = Self;

        fn serialize_bool(self, v: bool) -> Result<(), TokenError> {
            self.tokens.push(Token::Bool(v));
            Ok(())
        }

        fn serialize_i8(self, _v: i8) -> Result<(), TokenError> {
            Err(TokenError::custom("i8 is not used by T-04 DTOs"))
        }

        fn serialize_i16(self, _v: i16) -> Result<(), TokenError> {
            Err(TokenError::custom("i16 is not used by T-04 DTOs"))
        }

        fn serialize_i32(self, _v: i32) -> Result<(), TokenError> {
            Err(TokenError::custom("i32 is not used by T-04 DTOs"))
        }

        fn serialize_i64(self, _v: i64) -> Result<(), TokenError> {
            Err(TokenError::custom("i64 is not used by T-04 DTOs"))
        }

        fn serialize_i128(self, _v: i128) -> Result<(), TokenError> {
            Err(TokenError::custom("i128 is not used by T-04 DTOs"))
        }

        fn serialize_u8(self, v: u8) -> Result<(), TokenError> {
            self.tokens.push(Token::U8(v));
            Ok(())
        }

        fn serialize_u16(self, v: u16) -> Result<(), TokenError> {
            self.tokens.push(Token::U16(v));
            Ok(())
        }

        fn serialize_u32(self, v: u32) -> Result<(), TokenError> {
            self.tokens.push(Token::U32(v));
            Ok(())
        }

        fn serialize_u64(self, v: u64) -> Result<(), TokenError> {
            self.tokens.push(Token::U64(v));
            Ok(())
        }

        fn serialize_u128(self, _v: u128) -> Result<(), TokenError> {
            Err(TokenError::custom("u128 is not used by T-04 DTOs"))
        }

        fn serialize_f32(self, _v: f32) -> Result<(), TokenError> {
            Err(TokenError::custom("f32 is not used by T-04 DTOs"))
        }

        fn serialize_f64(self, _v: f64) -> Result<(), TokenError> {
            Err(TokenError::custom("f64 is not used by T-04 DTOs"))
        }

        fn serialize_char(self, _v: char) -> Result<(), TokenError> {
            Err(TokenError::custom("char is not used by T-04 DTOs"))
        }

        fn serialize_str(self, v: &str) -> Result<(), TokenError> {
            self.tokens.push(Token::Str(v.to_owned()));
            Ok(())
        }

        fn serialize_bytes(self, v: &[u8]) -> Result<(), TokenError> {
            // Byte buffers ride as `Bytes` tokens carrying the whole buffer; the deserializer
            // routes every `deserialize_*` hint through `deserialize_any`, so `&[u8]`/`Vec<u8>`
            // reads land on `visit_byte_buf` here.
            self.tokens.push(Token::Bytes(v.to_vec()));
            Ok(())
        }

        fn serialize_none(self) -> Result<(), TokenError> {
            self.tokens.push(Token::None);
            Ok(())
        }

        fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), TokenError> {
            self.tokens.push(Token::Some);
            value.serialize(&mut *self)
        }

        fn serialize_unit(self) -> Result<(), TokenError> {
            self.tokens.push(Token::Unit);
            Ok(())
        }

        fn serialize_unit_struct(self, _name: &'static str) -> Result<(), TokenError> {
            self.tokens.push(Token::Unit);
            Ok(())
        }

        fn serialize_unit_variant(
            self,
            _name: &'static str,
            _index: u32,
            variant: &'static str,
        ) -> Result<(), TokenError> {
            self.tokens.push(Token::Variant(variant.to_owned()));
            Ok(())
        }

        fn serialize_newtype_struct<T: Serialize + ?Sized>(
            self,
            _name: &'static str,
            value: &T,
        ) -> Result<(), TokenError> {
            // Newtype structs ride as a 1-tuple so the derive's tuple-struct visitor shape
            // matches on the way back (`deserialize_newtype_struct` consumes the marker).
            self.tokens.push(Token::Tuple(1));
            value.serialize(&mut *self)
        }

        fn serialize_newtype_variant<T: Serialize + ?Sized>(
            self,
            _name: &'static str,
            _index: u32,
            variant: &'static str,
            value: &T,
        ) -> Result<(), TokenError> {
            self.tokens.push(Token::Variant(variant.to_owned()));
            value.serialize(&mut *self)
        }

        fn serialize_seq(self, len: Option<usize>) -> Result<Self, TokenError> {
            let len = len.ok_or_else(|| TokenError::custom("sequence without length"))?;
            self.tokens.push(Token::Seq(len));
            Ok(self)
        }

        fn serialize_tuple(self, len: usize) -> Result<Self, TokenError> {
            self.tokens.push(Token::Tuple(len));
            Ok(self)
        }

        fn serialize_tuple_struct(
            self,
            _name: &'static str,
            len: usize,
        ) -> Result<Self, TokenError> {
            self.tokens.push(Token::Tuple(len));
            Ok(self)
        }

        fn serialize_tuple_variant(
            self,
            _name: &'static str,
            _index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, TokenError> {
            self.tokens.push(Token::Variant(variant.to_owned()));
            self.tokens.push(Token::Tuple(len));
            Ok(self)
        }

        fn serialize_map(self, len: Option<usize>) -> Result<Self, TokenError> {
            let len = len.ok_or_else(|| TokenError::custom("map without length"))?;
            self.tokens.push(Token::Seq(len * 2));
            Ok(self)
        }

        fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self, TokenError> {
            self.tokens.push(Token::Seq(len));
            Ok(self)
        }

        fn serialize_struct_variant(
            self,
            _name: &'static str,
            _index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, TokenError> {
            // Struct variants ride *without* an extra sequence marker: `Variant` + raw
            // values. `struct_variant` consumes values positionally via `BareSeqAccess`,
            // so no `Tuple(len)` token is emitted here.
            self.tokens.push(Token::Variant(variant.to_owned()));
            let _ = len;
            Ok(self)
        }
    }

    impl ser::SerializeSeq for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_element<T: Serialize + ?Sized>(
            &mut self,
            value: &T,
        ) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeTuple for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_element<T: Serialize + ?Sized>(
            &mut self,
            value: &T,
        ) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeTupleStruct for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeTupleVariant for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeMap for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), TokenError> {
            key.serialize(&mut **self)
        }

        fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeStruct for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_field<T: Serialize + ?Sized>(
            &mut self,
            _key: &'static str,
            value: &T,
        ) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    impl ser::SerializeStructVariant for &mut TokenSerializer {
        type Ok = ();
        type Error = TokenError;

        fn serialize_field<T: Serialize + ?Sized>(
            &mut self,
            _key: &'static str,
            value: &T,
        ) -> Result<(), TokenError> {
            value.serialize(&mut **self)
        }

        fn end(self) -> Result<(), TokenError> {
            Ok(())
        }
    }

    /// Reads tokens back.
    #[derive(Debug)]
    pub struct TokenDeserializer<'de> {
        tokens: &'de [Token],
    }

    impl<'de> TokenDeserializer<'de> {
        /// Borrows the token slice.
        #[must_use]
        pub fn new(tokens: &'de [Token]) -> Self {
            Self { tokens }
        }

        /// Whether every token was consumed.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.tokens.is_empty()
        }

        /// Unconsumed tokens, for diagnostics.
        #[must_use]
        pub fn rest(&self) -> &'de [Token] {
            self.tokens
        }

        fn next(&mut self) -> Result<&'de Token, TokenError> {
            match self.tokens.split_first() {
                Some((head, tail)) => {
                    self.tokens = tail;
                    Ok(head)
                }
                None => Err(TokenError::custom("unexpected end of tokens")),
            }
        }
    }

    /// Length-prefixed sequence access: stops after `remaining` elements, no end marker.
    struct SeqAccessImpl<'a, 'de> {
        de: &'a mut TokenDeserializer<'de>,
        remaining: usize,
    }

    impl<'de> SeqAccess<'de> for SeqAccessImpl<'_, 'de> {
        type Error = TokenError;

        fn next_element_seed<T: DeserializeSeed<'de>>(
            &mut self,
            seed: T,
        ) -> Result<Option<T::Value>, TokenError> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            seed.deserialize(&mut *self.de).map(Some)
        }
    }

    /// Length-prefixed map access.
    pub(crate) struct MapAccessImpl<'a, 'de> {
        pub(crate) de: &'a mut TokenDeserializer<'de>,
        pub(crate) remaining: usize,
    }

    impl<'de> MapAccess<'de> for MapAccessImpl<'_, 'de> {
        type Error = TokenError;

        fn next_key_seed<K: DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, TokenError> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            seed.deserialize(&mut *self.de).map(Some)
        }

        fn next_value_seed<V: DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, TokenError> {
            seed.deserialize(&mut *self.de)
        }
    }

    // `MapAccessImpl` is exercised through `probe_map_access` below: the harness never emits
    // `Map` markers itself (maps flatten to `Seq`), so the test drives the impl directly.
    #[cfg(test)]
    #[test]
    fn serde_token_map_access_shape() {
        use super::serde_token::{MapAccessImpl, Token, TokenDeserializer};
        use serde::de::MapAccess as _;

        struct PhantomU32;
        impl<'de> serde::de::DeserializeSeed<'de> for PhantomU32 {
            type Value = u32;
            fn deserialize<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<u32, D::Error> {
                serde::Deserialize::deserialize(deserializer)
            }
        }

        let tokens = vec![Token::Str(String::from("k")), Token::U32(7)];
        let mut de = TokenDeserializer::new(&tokens);
        let mut access = MapAccessImpl {
            de: &mut de,
            remaining: 1,
        };
        let key: String = access.next_key_seed(PhantomStr).unwrap().unwrap();
        assert_eq!(key, "k");
        let value: u32 = access.next_value_seed(PhantomU32).unwrap();
        assert_eq!(value, 7);
        let none: Option<String> = access.next_key_seed(PhantomStr).unwrap();
        assert_eq!(none, None);
    }

    /// Test seed reading a `String` from the token stream.
    #[cfg(all(test, feature = "serde"))]
    struct PhantomStr;

    #[cfg(all(test, feature = "serde"))]
    impl<'de> serde::de::DeserializeSeed<'de> for PhantomStr {
        type Value = String;
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<String, D::Error> {
            serde::Deserialize::deserialize(deserializer)
        }
    }

    /// A borrowed variant name presented as a deserializer for `variant_seed`.
    struct VariantName<'a>(&'a str);

    impl<'de> de::Deserializer<'de> for VariantName<'de> {
        type Error = TokenError;

        fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TokenError> {
            visitor.visit_str(self.0)
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }

    /// Enum access over a consumed variant name.
    struct EnumAccessImpl<'a, 'de> {
        de: &'a mut TokenDeserializer<'de>,
        variant: &'de str,
    }

    impl<'de> EnumAccess<'de> for EnumAccessImpl<'_, 'de> {
        type Error = TokenError;
        type Variant = Self;

        fn variant_seed<V: DeserializeSeed<'de>>(
            self,
            seed: V,
        ) -> Result<(V::Value, Self::Variant), TokenError> {
            let value = seed.deserialize(VariantName(self.variant))?;
            Ok((value, self))
        }
    }

    impl<'de> VariantAccess<'de> for EnumAccessImpl<'_, 'de> {
        type Error = TokenError;

        fn unit_variant(self) -> Result<(), TokenError> {
            Ok(())
        }

        fn newtype_variant_seed<T: DeserializeSeed<'de>>(
            self,
            seed: T,
        ) -> Result<T::Value, TokenError> {
            // Newtype tuple variants (`Put(CacheEntry)`, `PutRegistration(..)`) carry one
            // positional value: the harness encodes it transparently, so read the next token
            // directly.
            seed.deserialize(&mut *self.de)
        }

        fn tuple_variant<V: Visitor<'de>>(
            self,
            _len: usize,
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            // Tuple variants are length-prefixed (`Variant` + `Tuple(n)` + values): consume
            // the marker, then read positionally.
            match self.de.next()? {
                Token::Tuple(len) => {
                    let len = *len;
                    visitor.visit_seq(SeqAccessImpl {
                        de: self.de,
                        remaining: len,
                    })
                }
                other => Err(TokenError::custom(format!(
                    "expected tuple marker, found {other:?}"
                ))),
            }
        }

        fn struct_variant<V: Visitor<'de>>(
            self,
            _fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            // Struct variants ride *without* a length marker (`Variant` + raw values): hand
            // the visitor the bare token stream directly.
            visitor.visit_seq(BareSeqAccess { de: self.de })
        }
    }

    /// Sequence access over a bare token stream with no length marker: yields elements until
    /// the stream is exhausted. Only for struct-variant payloads, whose harness encoding is
    /// `Variant` + raw values.
    struct BareSeqAccess<'a, 'de> {
        de: &'a mut TokenDeserializer<'de>,
    }

    impl<'de> SeqAccess<'de> for BareSeqAccess<'_, 'de> {
        type Error = TokenError;

        fn next_element_seed<T: DeserializeSeed<'de>>(
            &mut self,
            seed: T,
        ) -> Result<Option<T::Value>, TokenError> {
            if self.de.is_empty() {
                return Ok(None);
            }
            seed.deserialize(&mut *self.de).map(Some)
        }
    }

    impl<'de> de::Deserializer<'de> for &mut TokenDeserializer<'de> {
        type Error = TokenError;

        /// Routes every concrete hint through `deserialize_any`: the token stream is
        /// self-describing, so `deserialize_u8` and friends must not demand a mismatched token
        /// kind — they only constrain how the visitor interprets the next token.
        fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TokenError> {
            match self.next()? {
                Token::Bool(v) => visitor.visit_bool(*v),
                Token::U8(v) => visitor.visit_u8(*v),
                Token::U16(v) => visitor.visit_u16(*v),
                Token::U32(v) => visitor.visit_u32(*v),
                Token::U64(v) => visitor.visit_u64(*v),
                Token::Str(s) => visitor.visit_string(s.clone()),
                Token::Bytes(b) => visitor.visit_byte_buf(b.clone()),
                Token::None => visitor.visit_none(),
                Token::Some => visitor.visit_some(&mut *self),
                Token::Unit => visitor.visit_unit(),
                Token::Seq(len) | Token::Tuple(len) | Token::Map(len) | Token::Struct(len) => {
                    let len = *len;
                    visitor.visit_seq(SeqAccessImpl {
                        de: self,
                        remaining: len,
                    })
                }
                Token::Variant(name) => visitor.visit_enum(EnumAccessImpl {
                    de: self,
                    variant: name,
                }),
            }
        }

        fn deserialize_enum<V: Visitor<'de>>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            match self.next()? {
                Token::Variant(name) => visitor.visit_enum(EnumAccessImpl {
                    de: self,
                    variant: name,
                }),
                other => Err(TokenError::custom(format!(
                    "expected variant, found {other:?}"
                ))),
            }
        }

        fn deserialize_newtype_struct<V: Visitor<'de>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            // Serializer side wraps the inner value in `Tuple(1)`: consume the marker, then
            // read the single value through the tuple-struct visitor shape.
            match self.next()? {
                Token::Tuple(1) => visitor.visit_seq(SeqAccessImpl {
                    de: self,
                    remaining: 1,
                }),
                other => Err(TokenError::custom(format!(
                    "expected 1-tuple marker, found {other:?}"
                ))),
            }
        }

        fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, TokenError> {
            match self.next()? {
                Token::U8(v) => visitor.visit_u8(*v),
                other => Err(TokenError::custom(format!("expected u8, found {other:?}"))),
            }
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct seq tuple
            tuple_struct map struct identifier ignored_any
        }
    }

    /// Serializes `value` to tokens and deserializes it back, asserting equality and exact
    /// token consumption.
    pub fn round_trip<T>(value: &T) -> T
    where
        T: Serialize + de::DeserializeOwned + PartialEq + fmt::Debug,
    {
        let mut serializer = TokenSerializer::default();
        value.serialize(&mut serializer).unwrap();
        let mut deserializer = TokenDeserializer::new(&serializer.tokens);
        let back = T::deserialize(&mut deserializer).unwrap();
        assert!(
            deserializer.is_empty(),
            "trailing tokens: {:?}",
            deserializer.rest()
        );
        assert_eq!(&back, value);
        back
    }

    /// Test probes exercising serializer arms no DTO uses directly (tuple-struct shape and
    /// unit-struct marker). Returns the observed tokens for assertion at the call site.
    #[cfg(test)]
    pub fn probe_tuple_struct() -> Vec<Token> {
        struct TupleStruct(u8, u16);
        impl Serialize for TupleStruct {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeTupleStruct as _;
                let mut state = serializer.serialize_tuple_struct("TupleStruct", 2)?;
                state.serialize_field(&self.0)?;
                state.serialize_field(&self.1)?;
                state.end()
            }
        }
        let mut serializer = TokenSerializer::default();
        TupleStruct(1, 2).serialize(&mut serializer).unwrap();
        serializer.tokens_for_test()
    }

    /// Test probe for the unit-struct serializer arm.
    #[cfg(test)]
    pub fn probe_unit_struct() -> Vec<Token> {
        struct UnitStruct;
        impl Serialize for UnitStruct {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_unit_struct("UnitStruct")
            }
        }
        let mut serializer = TokenSerializer::default();
        UnitStruct.serialize(&mut serializer).unwrap();
        serializer.tokens_for_test()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    fn worker_record() -> WorkerRecord {
        WorkerRecord {
            id: WorkerId::from_raw(1),
            registration: RegistrationId::from_raw(1),
            script_url: url("https://example.com/sw.js"),
            worker_type: WorkerType::Classic,
            state: WorkerState::Parsed,
            skip_waiting: false,
            imported_scripts_updated: false,
            has_fetch_handler: None,
            handled_event_types: SmallVec::new(),
            run_state: RunState::NotRunning,
            pending_events: 0,
            last_activity_ms: 0,
        }
    }

    fn registration_record() -> RegistrationRecord {
        RegistrationRecord {
            id: RegistrationId::from_raw(1),
            storage_key: StorageKey::from_raw("https://example.com"),
            scope: url("https://example.com/"),
            update_via_cache: UpdateViaCache::Imports,
            installing: None,
            waiting: None,
            active: None,
            last_update_check_ms: None,
            navigation_preload_enabled: false,
            navigation_preload_header: String::from("true"),
            uninstalling: false,
        }
    }

    #[test]
    fn defaults_are_spec_values() {
        let worker = worker_record();
        assert_eq!(worker.state, WorkerState::Parsed);
        assert!(!worker.skip_waiting);
        assert!(!worker.imported_scripts_updated);
        assert_eq!(worker.has_fetch_handler, None);
        assert!(worker.handled_event_types.is_empty());
        assert_eq!(worker.run_state, RunState::NotRunning);
        assert_eq!(worker.pending_events, 0);

        let registration = registration_record();
        assert_eq!(registration.update_via_cache, UpdateViaCache::Imports);
        assert!(!registration.navigation_preload_enabled);
        assert_eq!(registration.navigation_preload_header, "true");
        assert_eq!(registration.installing, None);
        assert_eq!(registration.waiting, None);
        assert_eq!(registration.active, None);
        assert!(!registration.uninstalling);
    }

    #[test]
    fn update_via_cache_default_is_imports() {
        assert_eq!(UpdateViaCache::default(), UpdateViaCache::Imports);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_token_harness_rejects_unused_types() {
        use super::serde_token::{Token, TokenDeserializer, TokenSerializer};
        use serde::Serializer as _;

        // Every "not used by T-04 DTOs" serializer arm returns an error, never a token.
        let mut serializer = TokenSerializer::default();
        assert!(serializer.serialize_i8(0).is_err());
        assert!(serializer.serialize_i16(0).is_err());
        assert!(serializer.serialize_i32(0).is_err());
        assert!(serializer.serialize_i64(0).is_err());
        assert!(serializer.serialize_i128(0).is_err());
        assert!(serializer.serialize_u128(0).is_err());
        assert!(serializer.serialize_f32(0.0).is_err());
        assert!(serializer.serialize_f64(0.0).is_err());
        assert!(serializer.serialize_char('a').is_err());
        assert!(serializer.serialize_seq(None).is_err());
        assert!(serializer.serialize_map(None).is_err());
        assert!(TokenDeserializer::new(&[]).is_empty());

        // `Token`/`TokenError` debug + display rendering is stable (used in diagnostics).
        let token = Token::Variant(String::from("Put"));
        assert_eq!(format!("{token:?}"), "Variant(\"Put\")");
        assert_eq!(TokenDeserializer::new(&[token]).rest().len(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_token_harness_remaining_arms() {
        use super::serde_token::{Token, TokenDeserializer};

        // Tuple / tuple-struct / tuple-variant / struct-variant arms produce the exact marker
        // tokens the deserializer consumes; map arm doubles key/value counts into one `Seq`.
        let (tuple_tokens, unit_tokens) = (
            super::serde_token::probe_tuple_struct(),
            super::serde_token::probe_unit_struct(),
        );
        assert_eq!(
            tuple_tokens,
            vec![Token::Tuple(2), Token::U8(1), Token::U16(2)]
        );
        assert_eq!(unit_tokens, vec![Token::Unit]);

        // `deserialize_any` on `Variant` + every error arm of the harness deserializer.
        let tokens = [Token::Variant(String::from("V"))];
        let mut de = TokenDeserializer::new(&tokens);
        let back = <String as serde::Deserialize>::deserialize(&mut de)
            .unwrap_err()
            .to_string();
        assert!(!back.is_empty());

        let empty: [Token; 0] = [];
        let mut de = TokenDeserializer::new(&empty);
        let err = <String as serde::Deserialize>::deserialize(&mut de)
            .unwrap_err()
            .to_string();
        assert!(err.contains("end of tokens"), "{err}");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_token_struct_shape() {
        // Full struct round-trip through `SeqAccess`: a two-field struct survives the
        // positional encoding, pinning the struct + `SeqAccess` + `next_element` path.
        serde_token_point_shape();
    }

    #[cfg(feature = "serde")]
    #[test]
    #[allow(clippy::too_many_lines)]
    fn serde_token_compound_arms_round_trip() {
        use super::serde_token::{Token, TokenDeserializer, TokenSerializer};

        // `SerializeSeq` / `SerializeTuple` / `SerializeMap` / `SerializeStruct` compound arms:
        // drive values through them and read them back through the matching token shapes.
        // (`Serializer` is implemented for `&mut TokenSerializer`, hence the reborrows.)
        let mut serializer = TokenSerializer::default();
        {
            let serializer_ref = &mut serializer;
            let mut seq =
                <&mut TokenSerializer as serde::Serializer>::serialize_seq(serializer_ref, Some(2))
                    .unwrap();
            <_ as serde::ser::SerializeSeq>::serialize_element(&mut seq, &1_u8).unwrap();
            <_ as serde::ser::SerializeSeq>::serialize_element(&mut seq, &2_u16).unwrap();
            <_ as serde::ser::SerializeSeq>::end(seq).unwrap();
        }
        let tokens = serializer.tokens_for_test();
        assert_eq!(tokens, vec![Token::Seq(2), Token::U8(1), Token::U16(2)]);
        // The `Seq(2)` marker is consumed by sequence-shaped reads (`SeqAccess` via structs
        // and tuples); scalar reads below consume the bare value tokens directly.
        let mut de = TokenDeserializer::new(&tokens[1..]);
        let first: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(first, 1);
        let tokens = serializer.tokens_for_test();
        let mut de = TokenDeserializer::new(&tokens[2..]);
        let second: u16 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(second, 2);
        assert!(de.is_empty());

        let mut serializer = TokenSerializer::default();
        {
            let serializer_ref = &mut serializer;
            let mut map =
                <&mut TokenSerializer as serde::Serializer>::serialize_map(serializer_ref, Some(1))
                    .unwrap();
            <_ as serde::ser::SerializeMap>::serialize_key(&mut map, "k").unwrap();
            <_ as serde::ser::SerializeMap>::serialize_value(&mut map, &7_u32).unwrap();
            <_ as serde::ser::SerializeMap>::end(map).unwrap();
        }
        // Map arm doubles key/value count into one flat `Seq(2)`.
        let tokens = serializer.tokens_for_test();
        assert_eq!(
            tokens,
            vec![Token::Seq(2), Token::Str(String::from("k")), Token::U32(7)]
        );

        // Struct arm is positional: fields land as a flat sequence, no field names.
        // `Point` pins the exact tokens (`Seq(2)` + two bare values) so the encoding stays
        // self-describing; see `serde_token_struct_shape` for the full struct round-trip.
        serde_token_point_tokens();
        // `u8` fields read through `deserialize_u8` on bare tokens.
        let bare = [Token::U8(3)];
        let mut de = TokenDeserializer::new(&bare);
        let x: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(x, 3);
        let bare = [Token::U8(4)];
        let mut de = TokenDeserializer::new(&bare);
        let y: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!((x, y), (3, 4));

        // `MapAccess`/`VariantName`/`variant_seed` arms: drive an enum + struct-variant
        // tokens. Only unit and struct variants are exercised: the newtype-variant seed path
        // is already covered by `StorageOp`/`CacheOperation` round-trips through `round_trip`.
        serde_token_probe_enum();
    }

    /// Tokens emitted by the positional struct encoding: `Seq(2)` + bare values.
    #[cfg(all(test, feature = "serde"))]
    fn serde_token_point_tokens() {
        use super::serde_token::{Token, TokenSerializer};
        use serde::ser::Serialize as _;

        struct Point {
            x: u8,
            y: u8,
        }
        impl serde::ser::Serialize for Point {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct as _;
                let mut state = serializer.serialize_struct("Point", 2)?;
                state.serialize_field("x", &self.x)?;
                state.serialize_field("y", &self.y)?;
                state.end()
            }
        }
        let mut serializer = TokenSerializer::default();
        Point { x: 3, y: 4 }.serialize(&mut serializer).unwrap();
        assert_eq!(
            serializer.tokens_for_test(),
            vec![Token::Seq(2), Token::U8(3), Token::U8(4)]
        );
    }

    /// Reads a two-field point from a positional sequence.
    #[cfg(all(test, feature = "serde"))]
    fn serde_token_point_from_seq<'a, A>(mut seq: A) -> Result<(u8, u8), A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let x: u8 = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::custom("missing x"))?;
        let y: u8 = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::custom("missing y"))?;
        Ok((x, y))
    }

    /// Two-field struct round-trip through the positional struct encoding.
    #[cfg(all(test, feature = "serde"))]
    fn serde_token_point_shape() {
        use super::serde_token::{Token, TokenSerializer, round_trip};
        use serde::ser::Serialize as _;

        #[derive(Debug, PartialEq)]
        struct Point {
            x: u8,
            y: u8,
        }
        impl serde::ser::Serialize for Point {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeStruct as _;
                let mut state = serializer.serialize_struct("Point", 2)?;
                state.serialize_field("x", &self.x)?;
                state.serialize_field("y", &self.y)?;
                state.end()
            }
        }
        impl<'de> serde::Deserialize<'de> for Point {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct PointVisitor;
                impl<'de> serde::de::Visitor<'de> for PointVisitor {
                    type Value = Point;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("Point with two fields")
                    }
                    fn visit_seq<A: serde::de::SeqAccess<'de>>(
                        self,
                        seq: A,
                    ) -> Result<Point, A::Error> {
                        let (x, y) = crate::model::tests::serde_token_point_from_seq(seq)?;
                        Ok(Point { x, y })
                    }
                }
                deserializer.deserialize_struct("Point", &["x", "y"], PointVisitor)
            }
        }
        let mut serializer = TokenSerializer::default();
        Point { x: 3, y: 4 }.serialize(&mut serializer).unwrap();
        assert_eq!(
            serializer.tokens_for_test(),
            vec![Token::Seq(2), Token::U8(3), Token::U8(4)]
        );
        assert_eq!(round_trip(&Point { x: 3, y: 4 }), Point { x: 3, y: 4 });
    }

    /// Probe enum exercising the harness's enum + struct-variant path (`VariantName`,
    /// `variant_seed`, `BareSeqAccess`). Only unit and struct variants: the newtype-variant
    /// seed path is already covered by `StorageOp`/`CacheOperation` round-trips.
    #[cfg(all(test, feature = "serde"))]
    #[allow(clippy::too_many_lines)]
    fn serde_token_probe_enum() {
        use super::serde_token::{TokenDeserializer, round_trip};

        enum Probe {
            Unit,
            Struct { a: u8 },
        }
        impl serde::ser::Serialize for Probe {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                match self {
                    Probe::Unit => serializer.serialize_unit_variant("Probe", 0, "Unit"),
                    Probe::Struct { a } => {
                        use serde::ser::SerializeStructVariant as _;
                        let mut state =
                            serializer.serialize_struct_variant("Probe", 1, "Struct", 1)?;
                        state.serialize_field("a", a)?;
                        state.end()
                    }
                }
            }
        }
        impl<'de> serde::Deserialize<'de> for Probe {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let data: (ProbeTag, ProbePayload) =
                    deserializer.deserialize_enum("Probe", &["Unit", "Struct"], ProbeVisitor)?;
                Ok(match data {
                    (ProbeTag::Unit, ProbePayload::Unit(())) => Probe::Unit,
                    (ProbeTag::Struct, ProbePayload::Struct(a)) => Probe::Struct { a },
                    _ => return Err(serde::de::Error::custom("tag/payload mismatch")),
                })
            }
        }
        struct ProbeVisitor;
        enum ProbeTag {
            Unit,
            Struct,
        }
        enum ProbePayload {
            Unit(()),
            Struct(u8),
        }
        struct OneU8Seed;
        impl serde::de::Visitor<'_> for OneU8Seed {
            type Value = u8;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("u8")
            }
            fn visit_u8<E: serde::de::Error>(self, v: u8) -> Result<u8, E> {
                Ok(v)
            }
        }
        struct ProbeStructVisitor;
        impl<'a> serde::de::Visitor<'a> for ProbeStructVisitor {
            type Value = u8;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("struct variant with one bare field")
            }
            #[allow(clippy::needless_lifetimes)]
            fn visit_seq<A: serde::de::SeqAccess<'a>>(self, mut seq: A) -> Result<u8, A::Error> {
                // `BareSeqAccess` yields raw payload tokens: read the single `U8` through
                // its own hint so no sequence shape is demanded.
                let value: u8 = seq
                    .next_element_seed(BareU8)?
                    .ok_or_else(|| serde::de::Error::custom("empty"))?;
                Ok(value)
            }
        }
        struct BareU8;
        impl<'a> serde::de::DeserializeSeed<'a> for BareU8 {
            type Value = u8;
            fn deserialize<D: serde::Deserializer<'a>>(
                self,
                deserializer: D,
            ) -> Result<u8, D::Error> {
                serde::Deserializer::deserialize_u8(deserializer, OneU8Seed)
            }
        }
        impl<'de> serde::de::Visitor<'de> for ProbeVisitor {
            type Value = (ProbeTag, ProbePayload);
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Probe variant with payload")
            }
            fn visit_enum<A: serde::de::EnumAccess<'de>>(
                self,
                data: A,
            ) -> Result<(ProbeTag, ProbePayload), A::Error> {
                use serde::de::VariantAccess as _;
                let (tag, variant) = data.variant_seed(ProbeTagSeed)?;
                match tag {
                    ProbeTag::Unit => variant
                        .unit_variant()
                        .map(|()| (ProbeTag::Unit, ProbePayload::Unit(()))),
                    ProbeTag::Struct => variant
                        .struct_variant(&["a"], ProbeStructVisitor)
                        .map(|a| (ProbeTag::Struct, ProbePayload::Struct(a))),
                }
            }
        }
        struct ProbeTagSeed;
        impl<'de> serde::de::DeserializeSeed<'de> for ProbeTagSeed {
            type Value = ProbeTag;
            fn deserialize<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<ProbeTag, D::Error> {
                struct TagVisitor;
                impl serde::de::Visitor<'_> for TagVisitor {
                    type Value = ProbeTag;
                    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.write_str("Probe variant")
                    }
                    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<ProbeTag, E> {
                        match v {
                            "Unit" => Ok(ProbeTag::Unit),
                            "Struct" => Ok(ProbeTag::Struct),
                            other => Err(serde::de::Error::unknown_variant(
                                other,
                                &["Unit", "Struct"],
                            )),
                        }
                    }
                }
                deserializer.deserialize_str(TagVisitor)
            }
        }
        impl PartialEq for Probe {
            fn eq(&self, other: &Self) -> bool {
                match (self, other) {
                    (Probe::Unit, Probe::Unit) => true,
                    (Probe::Struct { a: lhs }, Probe::Struct { a: rhs }) => lhs == rhs,
                    _ => false,
                }
            }
        }
        impl std::fmt::Debug for Probe {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    Probe::Unit => f.write_str("Unit"),
                    Probe::Struct { a } => f.debug_struct("Struct").field("a", a).finish(),
                }
            }
        }
        // `OneU8Seed` exercises the bare-`U8`-token read path used by transparent newtypes.
        let tokens = [super::serde_token::Token::U8(7)];
        let mut de = TokenDeserializer::new(&tokens);
        let back: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(back, 7);
        let _ = OneU8Seed;
        assert_eq!(round_trip(&Probe::Unit), Probe::Unit);
        assert_eq!(round_trip(&Probe::Struct { a: 5 }), Probe::Struct { a: 5 });
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trip_records() {
        use super::serde_token::round_trip;

        let mut worker = worker_record();
        worker.handled_event_types = SmallVec::from_vec(vec![
            EventType::Install,
            EventType::Fetch,
            EventType::Message,
        ]);
        worker.has_fetch_handler = Some(true);
        let back = round_trip(&worker);
        assert_eq!(back, worker);

        let mut registration = registration_record();
        registration.installing = Some(WorkerId::from_raw(1));
        registration.last_update_check_ms = Some(1_700_000_000_000);
        let back = round_trip(&registration);
        assert_eq!(back, registration);

        let resource = ScriptResource {
            url: url("https://example.com/sw.js"),
            bytes: Rc::from(b"console.log(1);".as_slice()),
            sha256: [9; 32],
            mime: String::from("text/javascript"),
            service_worker_allowed: Some(String::from("/")),
            is_main: true,
        };
        let back = round_trip(&resource);
        assert_eq!(back, resource);

        for state in [
            WorkerState::Parsed,
            WorkerState::Installing,
            WorkerState::Installed,
            WorkerState::Activating,
            WorkerState::Activated,
            WorkerState::Redundant,
        ] {
            assert_eq!(round_trip(&state), state);
        }
        for worker_type in [WorkerType::Classic, WorkerType::Module] {
            assert_eq!(round_trip(&worker_type), worker_type);
        }
        for via_cache in [
            UpdateViaCache::Imports,
            UpdateViaCache::All,
            UpdateViaCache::None,
        ] {
            assert_eq!(round_trip(&via_cache), via_cache);
        }
        for run_state in [
            RunState::NotRunning,
            RunState::Starting,
            RunState::Running,
            RunState::Terminating,
        ] {
            assert_eq!(round_trip(&run_state), run_state);
        }
        for event_type in [
            EventType::Install,
            EventType::Activate,
            EventType::Fetch,
            EventType::Message,
            EventType::Push,
            EventType::Sync,
            EventType::NotificationClick,
            EventType::NotificationClose,
        ] {
            assert_eq!(round_trip(&event_type), event_type);
        }
    }

    #[test]
    fn model_records_debug_is_stable() {
        // `insta` snapshot of the `Debug` rendering: adding/removing a field breaks the
        // snapshot loudly, which is the observable part of the §3 API contract this task can
        // pin without a serde-capable test serializer.
        insta::assert_debug_snapshot!(worker_record(), @"WorkerRecord {
    id: WorkerId(
        1,
    ),
    registration: RegistrationId(
        1,
    ),
    script_url: Url {
        scheme: \"https\",
        cannot_be_a_base: false,
        username: \"\",
        password: None,
        host: Some(
            Domain(
                \"example.com\",
            ),
        ),
        port: None,
        path: \"/sw.js\",
        query: None,
        fragment: None,
    },
    worker_type: Classic,
    state: Parsed,
    skip_waiting: false,
    imported_scripts_updated: false,
    has_fetch_handler: None,
    handled_event_types: [],
    run_state: NotRunning,
    pending_events: 0,
    last_activity_ms: 0,
}");
        insta::assert_debug_snapshot!(registration_record(), @"RegistrationRecord {
    id: RegistrationId(
        1,
    ),
    storage_key: StorageKey(
        \"https://example.com\",
    ),
    scope: Url {
        scheme: \"https\",
        cannot_be_a_base: false,
        username: \"\",
        password: None,
        host: Some(
            Domain(
                \"example.com\",
            ),
        ),
        port: None,
        path: \"/\",
        query: None,
        fragment: None,
    },
    update_via_cache: Imports,
    installing: None,
    waiting: None,
    active: None,
    last_update_check_ms: None,
    navigation_preload_enabled: false,
    navigation_preload_header: \"true\",
    uninstalling: false,
}");
    }

    #[test]
    fn script_resource_map_preserves_absolute_url_keys() {
        let mut map: ScriptResourceMap = IndexMap::new();
        let first = ScriptResource {
            url: url("https://example.com/a.js"),
            bytes: Rc::from(b"a".as_slice()),
            sha256: [1; 32],
            mime: String::from("text/javascript"),
            service_worker_allowed: None,
            is_main: true,
        };
        let second = ScriptResource {
            url: url("https://example.com/b.js"),
            bytes: Rc::from(b"bb".as_slice()),
            sha256: [2; 32],
            mime: String::from("application/javascript"),
            service_worker_allowed: Some(String::from("/")),
            is_main: false,
        };
        map.insert(first.url.clone(), first);
        map.insert(second.url.clone(), second);

        assert_eq!(map.len(), 2);
        let main = &map[&url("https://example.com/a.js")];
        assert!(main.is_main);
        assert_eq!(main.mime, "text/javascript");
        assert_eq!(&*main.bytes, b"a");
        assert_eq!(main.sha256, [1; 32]);
        // Insertion order preserved.
        let keys: Vec<&Url> = map.keys().collect();
        assert_eq!(
            keys,
            [
                &url("https://example.com/a.js"),
                &url("https://example.com/b.js")
            ]
        );
    }
}
