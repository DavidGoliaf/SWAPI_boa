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

/// `SmallVec<[EventType; 8]>` as a plain sequence on the wire.
///
/// `smallvec`'s own `serde` impls live behind its `serde` feature, which this crate does not
/// enable (the `serde` feature here must stay dependency-free): `collect_seq`/`Vec` round-trip
/// needs no `smallvec` impls at all.
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

/// Test-only `serde` round-trip helper shared by this module's and `storage`'s tests.
///
/// No format crate is involved: `Serialize` drives tokens into a `Vec<Token>` and `Deserialize`
/// reads them back, proving both impls agree. Structs ride positionally (`Seq(n)` + values, no
/// field names); newtype structs are transparent; enum variants ride as `Variant(name)` with
/// newtype/tuple payloads inline and struct-variant payloads as bare values.
#[cfg(all(test, feature = "serde"))]
pub(crate) mod serde_token {
    use std::fmt;

    use serde::de::{self, DeserializeSeed, EnumAccess, SeqAccess, VariantAccess, Visitor};
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
        /// Sequence of exactly `n` elements (structs, seqs, tuples, maps as `2 * len`).
        Seq(usize),
        /// Enum variant name.
        Variant(String),
    }

    /// Harness error.
    #[derive(Clone, Debug, PartialEq)]
    pub struct TokenError(pub String);

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

    /// Drives `Serialize` into tokens. Scalar arms the T-04 DTOs never emit
    /// (`i8`–`i128`, `u128`, floats, `char`, length-less sequences/maps) are rejected.
    #[derive(Debug, Default)]
    pub struct TokenSerializer {
        tokens: Vec<Token>,
    }

    impl TokenSerializer {
        /// Cloned tokens, for tests that serialize and deserialize in two steps.
        pub fn tokens_for_test(&self) -> Vec<Token> {
            self.tokens.clone()
        }
    }

    /// One compound-value helper per method spelling: `element` for `Seq`/`Tuple`,
    /// positional `field` for tuple structs/variants. (`Map`/`Struct`/`StructVariant` need
    /// distinct `key`/`value`/`_key` shapes and stay written out below.)
    macro_rules! impl_seq_like {
        ($($t:ty),*) => {$(
            impl $t for &mut TokenSerializer {
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
        )*};
    }
    macro_rules! impl_field_like {
        ($($t:ty),*) => {$(
            impl $t for &mut TokenSerializer {
                type Ok = ();
                type Error = TokenError;
                fn serialize_field<T: Serialize + ?Sized>(
                    &mut self,
                    value: &T,
                ) -> Result<(), TokenError> {
                    value.serialize(&mut **self)
                }
                fn end(self) -> Result<(), TokenError> {
                    Ok(())
                }
            }
        )*};
    }
    impl_seq_like!(ser::SerializeSeq, ser::SerializeTuple);
    impl_field_like!(ser::SerializeTupleStruct, ser::SerializeTupleVariant);

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

    /// Scalar arms: used ones push tokens, the rest reject. Written out (not macro-generated
    /// bodies) so each arm's encoding stays visible in review.
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

        /// Transparent newtype encoding: the inner value's tokens follow directly.
        /// (`#[serde(transparent)]` on the id/key newtypes means no wrapper is emitted, so
        /// none is consumed here either.)
        fn serialize_newtype_struct<T: Serialize + ?Sized>(
            self,
            _name: &'static str,
            value: &T,
        ) -> Result<(), TokenError> {
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

        /// Tuples ride as plain sequences; only newtype *variants* keep a `Variant` marker.
        fn serialize_tuple(self, len: usize) -> Result<Self, TokenError> {
            self.tokens.push(Token::Seq(len));
            Ok(self)
        }

        fn serialize_tuple_struct(
            self,
            _name: &'static str,
            len: usize,
        ) -> Result<Self, TokenError> {
            self.tokens.push(Token::Seq(len));
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
            self.tokens.push(Token::Seq(len));
            Ok(self)
        }

        /// Maps flatten to `Seq(2 * len)`: keys and values interleave, no field names.
        fn serialize_map(self, len: Option<usize>) -> Result<Self, TokenError> {
            let len = len.ok_or_else(|| TokenError::custom("map without length"))?;
            self.tokens.push(Token::Seq(len * 2));
            Ok(self)
        }

        fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self, TokenError> {
            self.tokens.push(Token::Seq(len));
            Ok(self)
        }

        /// Struct variants ride *without* a length marker (`Variant` + raw values); the
        /// deserializer consumes them via `BareSeqAccess` below.
        fn serialize_struct_variant(
            self,
            _name: &'static str,
            _index: u32,
            variant: &'static str,
            len: usize,
        ) -> Result<Self, TokenError> {
            self.tokens.push(Token::Variant(variant.to_owned()));
            let _ = len;
            Ok(self)
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

    /// Sequence access over a bare token stream with no length marker: yields elements until
    /// the stream is exhausted. Only for struct-variant payloads (`Variant` + raw values).
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

        /// Newtype variants carry one transparently-encoded positional value.
        fn newtype_variant_seed<T: DeserializeSeed<'de>>(
            self,
            seed: T,
        ) -> Result<T::Value, TokenError> {
            seed.deserialize(&mut *self.de)
        }

        /// Tuple variants are length-prefixed (`Variant` + `Seq(n)` + values).
        fn tuple_variant<V: Visitor<'de>>(
            self,
            _len: usize,
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            match self.de.next()? {
                Token::Seq(len) => {
                    let len = *len;
                    visitor.visit_seq(SeqAccessImpl {
                        de: self.de,
                        remaining: len,
                    })
                }
                other => Err(TokenError::custom(format!(
                    "expected sequence marker, found {other:?}"
                ))),
            }
        }

        fn struct_variant<V: Visitor<'de>>(
            self,
            _fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            visitor.visit_seq(BareSeqAccess { de: self.de })
        }
    }

    impl<'de> de::Deserializer<'de> for &mut TokenDeserializer<'de> {
        type Error = TokenError;

        /// The token stream is self-describing: every concrete hint routes here, and the next
        /// token decides how the visitor is fed. The only dedicated hint is `deserialize_u8`,
        /// so bare `U8` tokens survive struct-variant payloads without demanding a sequence.
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
                Token::Seq(len) => {
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

        /// Transparent newtype encoding: the inner value's tokens follow directly.
        fn deserialize_newtype_struct<V: Visitor<'de>>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, TokenError> {
            self.deserialize_any(visitor)
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

    /// Drives a probe through one serializer arm and returns the emitted tokens.
    pub fn tokens_of<F>(emit: F) -> Vec<Token>
    where
        F: FnOnce(&mut TokenSerializer),
    {
        let mut serializer = TokenSerializer::default();
        emit(&mut serializer);
        serializer.tokens_for_test()
    }

    /// Length-prefixed map access, exposed for its dedicated shape test below (the
    /// harness itself flattens maps to `Seq`, so no DTO drives this impl directly).
    pub(crate) struct MapAccessImpl<'a, 'de> {
        pub(crate) de: &'a mut TokenDeserializer<'de>,
        pub(crate) remaining: usize,
    }

    impl<'de> de::MapAccess<'de> for MapAccessImpl<'_, 'de> {
        type Error = TokenError;

        fn next_key_seed<K: de::DeserializeSeed<'de>>(
            &mut self,
            seed: K,
        ) -> Result<Option<K::Value>, TokenError> {
            if self.remaining == 0 {
                return Ok(None);
            }
            self.remaining -= 1;
            seed.deserialize(&mut *self.de).map(Some)
        }

        fn next_value_seed<V: de::DeserializeSeed<'de>>(
            &mut self,
            seed: V,
        ) -> Result<V::Value, TokenError> {
            seed.deserialize(&mut *self.de)
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_token_tests {
    use super::serde_token::{Token, TokenDeserializer, TokenSerializer};
    use serde::Serializer as _;

    /// Every unused scalar arm rejects instead of emitting.
    #[test]
    fn rejects_unused_types() {
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

        let token = Token::Variant(String::from("Put"));
        assert_eq!(format!("{token:?}"), "Variant(\"Put\")");
        assert_eq!(TokenDeserializer::new(&[token]).rest().len(), 1);
    }

    /// Marker arms (tuple/tuple-struct/unit-struct) plus deserializer error arms.
    #[test]
    fn remaining_arms() {
        use super::serde_token::tokens_of;
        use serde::ser::Serialize as _;

        assert_eq!(
            tokens_of(|s| {
                struct TupleStruct(u8, u16);
                impl serde::ser::Serialize for TupleStruct {
                    fn serialize<S: serde::Serializer>(
                        &self,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error> {
                        use serde::ser::SerializeTupleStruct as _;
                        let mut state = serializer.serialize_tuple_struct("TupleStruct", 2)?;
                        state.serialize_field(&self.0)?;
                        state.serialize_field(&self.1)?;
                        state.end()
                    }
                }
                TupleStruct(1, 2).serialize(s).unwrap();
            }),
            vec![Token::Seq(2), Token::U8(1), Token::U16(2)]
        );
        assert_eq!(
            tokens_of(|s| {
                struct UnitStruct;
                impl serde::ser::Serialize for UnitStruct {
                    fn serialize<S: serde::Serializer>(
                        &self,
                        serializer: S,
                    ) -> Result<S::Ok, S::Error> {
                        serializer.serialize_unit_struct("UnitStruct")
                    }
                }
                UnitStruct.serialize(s).unwrap();
            }),
            vec![Token::Unit]
        );

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

    /// Compound arms (`Seq`/`Map`/`Struct`/struct-variant probe) round-trip.
    #[test]
    #[allow(clippy::too_many_lines)]
    fn compound_arms_round_trip() {
        use super::serde_token::{TokenDeserializer, tokens_of};

        let tokens = tokens_of(|s| {
            let serializer_ref = s;
            let mut seq =
                <&mut TokenSerializer as serde::Serializer>::serialize_seq(serializer_ref, Some(2))
                    .unwrap();
            <_ as serde::ser::SerializeSeq>::serialize_element(&mut seq, &1_u8).unwrap();
            <_ as serde::ser::SerializeSeq>::serialize_element(&mut seq, &2_u16).unwrap();
            <_ as serde::ser::SerializeSeq>::end(seq).unwrap();
        });
        assert_eq!(tokens, vec![Token::Seq(2), Token::U8(1), Token::U16(2)]);
        let mut de = TokenDeserializer::new(&tokens[1..]);
        let first: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(first, 1);
        let mut de = TokenDeserializer::new(&tokens[2..]);
        let second: u16 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(second, 2);
        assert!(de.is_empty());

        let tokens = tokens_of(|s| {
            let serializer_ref = s;
            let mut map =
                <&mut TokenSerializer as serde::Serializer>::serialize_map(serializer_ref, Some(1))
                    .unwrap();
            <_ as serde::ser::SerializeMap>::serialize_key(&mut map, "k").unwrap();
            <_ as serde::ser::SerializeMap>::serialize_value(&mut map, &7_u32).unwrap();
            <_ as serde::ser::SerializeMap>::end(map).unwrap();
        });
        assert_eq!(
            tokens,
            vec![Token::Seq(2), Token::Str(String::from("k")), Token::U32(7)]
        );

        // Structs are positional (`Seq(2)` + bare values, no field names).
        assert_eq!(
            super::serde_token_tests::point_tokens(),
            vec![Token::Seq(2), Token::U8(3), Token::U8(4)]
        );
        let bare = [Token::U8(3)];
        let mut de = TokenDeserializer::new(&bare);
        let x: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(x, 3);
        let bare = [Token::U8(4)];
        let mut de = TokenDeserializer::new(&bare);
        let y: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!((x, y), (3, 4));

        // Enum + struct-variant path (`VariantName`, `variant_seed`, `BareSeqAccess`).
        super::serde_token_tests::probe_enum();
    }

    /// Tokens emitted by the positional struct encoding: `Seq(2)` + bare values.
    #[cfg(all(test, feature = "serde"))]
    pub(super) fn point_tokens() -> Vec<Token> {
        use super::serde_token::tokens_of;

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
        tokens_of(|s| {
            use serde::ser::Serialize as _;
            Point { x: 3, y: 4 }.serialize(s).unwrap();
        })
    }

    /// Two-field struct round-trip through the positional struct encoding.
    #[test]
    fn struct_shape() {
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
                        mut seq: A,
                    ) -> Result<Point, A::Error> {
                        let x: u8 = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::custom("missing x"))?;
                        let y: u8 = seq
                            .next_element()?
                            .ok_or_else(|| serde::de::Error::custom("missing y"))?;
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

    /// Length-prefixed map access driven directly (the harness flattens maps to `Seq`).
    #[test]
    fn map_access_shape() {
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

        struct PhantomStr;
        impl<'de> serde::de::DeserializeSeed<'de> for PhantomStr {
            type Value = String;
            fn deserialize<D: serde::Deserializer<'de>>(
                self,
                deserializer: D,
            ) -> Result<String, D::Error> {
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

    /// Probe enum exercising the harness's enum + struct-variant path.
    #[allow(clippy::too_many_lines)]
    pub(super) fn probe_enum() {
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
        let tokens = [super::serde_token::Token::U8(7)];
        let mut de = TokenDeserializer::new(&tokens);
        let back: u8 = serde::Deserialize::deserialize(&mut de).unwrap();
        assert_eq!(back, 7);
        let _ = OneU8Seed;
        assert_eq!(round_trip(&Probe::Unit), Probe::Unit);
        assert_eq!(round_trip(&Probe::Struct { a: 5 }), Probe::Struct { a: 5 });
    }

    /// One round-trip per record/DTO family plus every enum variant (parent work order §7).
    #[test]
    fn round_trip_records() {
        use super::serde_token::round_trip;
        use super::{
            EventType, RegistrationId, RegistrationRecord, RunState, ScriptResource, StorageKey,
            UpdateViaCache, WorkerId, WorkerRecord, WorkerState, WorkerType,
        };
        use smallvec::SmallVec;
        use std::rc::Rc;
        use url::Url;

        fn url(s: &str) -> Url {
            Url::parse(s).unwrap()
        }

        let worker = WorkerRecord {
            id: WorkerId::from_raw(1),
            registration: RegistrationId::from_raw(1),
            script_url: url("https://example.com/sw.js"),
            worker_type: WorkerType::Classic,
            state: WorkerState::Parsed,
            skip_waiting: false,
            imported_scripts_updated: false,
            has_fetch_handler: Some(true),
            handled_event_types: SmallVec::from_vec(vec![
                EventType::Install,
                EventType::Fetch,
                EventType::Message,
            ]),
            run_state: RunState::NotRunning,
            pending_events: 0,
            last_activity_ms: 0,
        };
        assert_eq!(round_trip(&worker), worker);

        let registration = RegistrationRecord {
            id: RegistrationId::from_raw(1),
            storage_key: StorageKey::from_raw("https://example.com"),
            scope: url("https://example.com/"),
            update_via_cache: UpdateViaCache::Imports,
            installing: Some(WorkerId::from_raw(1)),
            waiting: None,
            active: None,
            last_update_check_ms: Some(1_700_000_000_000),
            navigation_preload_enabled: false,
            navigation_preload_header: String::from("true"),
            uninstalling: false,
        };
        assert_eq!(round_trip(&registration), registration);

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
}
