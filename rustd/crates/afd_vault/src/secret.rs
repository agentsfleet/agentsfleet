//! The two values a vault write is made of, each valid by construction.
//!
//! # Parse, do not validate
//!
//! `secrets.zig` calls `validateSecretName` and `vault.validateObject` at the
//! top of `innerStoreSecret` and again at the top of `innerReplaceSecret`, and
//! a third verb that forgot either would compile and store an ambiguous shape.
//! Here the checks ARE the constructors: a [`SecretName`] cannot be empty or
//! over-long, and a [`SecretBody`] cannot be anything but a non-empty JSON
//! object within its bound. Every function downstream takes those types, so
//! there is no re-check to remember and none to delete (`M-STRONG-TYPES-GUARD`).
//!
//! # One parse produces the ciphertext AND the projection
//!
//! [`SecretBody`] holds the canonical plaintext and the [`Projection`] of that
//! same plaintext, built together in one constructor. No caller can supply a
//! projection, because none can construct one — which is what makes "the
//! `meta_*` columns describe the ciphertext beside them" a fact about the type
//! rather than a promise about the statement. `state/vault.zig` gets there by
//! being the only writer and saying so in a comment; this gets there by leaving
//! no other shape expressible.

use afd_crypto::secret::SecretBytes;
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::error::{ErrorKind, Result};
use crate::projection::Projection;

/// The most bytes a secret name may carry — `MAX_SECRET_NAME_LEN`.
///
/// Crate-private where [`MAX_DATA_BYTES`] is not, and the asymmetry is real
/// rather than an oversight: a caller cannot compose a body without knowing the
/// size bound, and the router suite asserts against it. A NAME is refused by
/// [`SecretName::parse`] with a sentence that states the bound, so nobody
/// outside needs the number.
pub(crate) const MAX_NAME_BYTES: usize = 64;

/// The most bytes a stringified secret body may carry — `MAX_SECRET_DATA_LEN`.
///
/// Measured on the CANONICAL form this crate produces, not on the request body,
/// so the same object is accepted or refused identically however the caller
/// spaced it.
pub const MAX_DATA_BYTES: usize = 4 * 1024;

/// A name a workspace may store a secret under.
///
/// Bounded by LENGTH alone, which is what `validateSecretName` bounds. A
/// stricter alphabet is tempting — the name is interpolated as
/// `${secrets.<name>.<field>}` and rides a path segment — but rows written by
/// the Zig daemon already hold whatever it accepted, and a Rust daemon that
/// refused those names would make existing credentials unreachable through the
/// item route mid-cutover. Parity is the milestone's rule and it applies here.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecretName(Box<str>);

impl SecretName {
    /// Reads a name, refusing an empty one and one past the bound.
    ///
    /// # Errors
    /// Refuses a name of zero bytes and one over [`MAX_NAME_BYTES`].
    pub fn parse(raw: &str) -> Result<Self> {
        // BYTES, not characters. The column is `TEXT` and the Zig bound is
        // `name.len`, which is a byte count; counting characters would accept a
        // name the other daemon refuses.
        if raw.is_empty() || raw.len() > MAX_NAME_BYTES {
            return Err(ErrorKind::NameInvalid.into());
        }
        Ok(Self(raw.into()))
    }

    /// The name as stored, and as the associated data binds it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A secret body ready to be sealed, with the projection of those same bytes.
///
/// The plaintext is wrapped in [`SecretBytes`], so it is zeroed when this value
/// drops rather than left on the heap for whatever allocates next — the
/// property `secure_memory.freeBytes` gives the Zig write path, obtained here
/// from a destructor instead of from a `defer` somebody has to write.
#[derive(Debug)]
pub struct SecretBody {
    plaintext: SecretBytes,
    projection: Projection,
}

impl SecretBody {
    /// Reads the `data` field of a create or replace request.
    ///
    /// Takes the raw JSON rather than a parsed tree so the ONE parse that
    /// happens is this one, and both the bytes that get sealed and the
    /// projection that gets written come out of it.
    ///
    /// # Errors
    /// Refuses anything that is not a JSON object, an object with no keys, and
    /// a canonical form past [`MAX_DATA_BYTES`].
    pub fn parse(data: &RawValue) -> Result<Self> {
        // A map, requested as a map: `serde_json` answers this with `{` alone,
        // so an array, a scalar and a null are all refused here rather than by
        // a shape check after the fact. Same guarantee `afd_core::json` gives a
        // derived struct, and for the same reason.
        let object: Map<String, Value> =
            serde_json::from_str(data.get()).map_err(|_not_an_object| ErrorKind::DataInvalid)?;
        if object.is_empty() {
            return Err(ErrorKind::DataInvalid.into());
        }

        // Canonical, and canonical the same way the Zig daemon is: the
        // workspace enables `serde_json/preserve_order`, so a key set keeps the
        // order it arrived in — which is what `std.json.Value`'s
        // insertion-ordered object map does. Two daemons stringifying one body
        // therefore produce the same bytes, and the size bound below decides
        // identically on either.
        let canonical =
            serde_json::to_vec(&object).map_err(|_unwritable| ErrorKind::DataInvalid)?;
        if canonical.len() > MAX_DATA_BYTES {
            return Err(ErrorKind::DataTooLarge.into());
        }

        Ok(Self {
            projection: Projection::of(&object),
            plaintext: SecretBytes::new(canonical),
        })
    }

    /// The canonical bytes that get sealed.
    pub(crate) fn plaintext(&self) -> &[u8] {
        self.plaintext.expose()
    }

    /// The projection of those same bytes.
    pub(crate) const fn projection(&self) -> &Projection {
        &self.projection
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::{MAX_DATA_BYTES, MAX_NAME_BYTES, SecretBody, SecretName};
    use crate::projection::Kind;
    use serde_json::value::RawValue;

    /// The `data` field as a handler holds it, straight off the request.
    fn data(json: &str) -> Box<RawValue> {
        RawValue::from_string(json.to_owned()).expect("the fixture is valid JSON")
    }

    #[test]
    fn a_name_is_bounded_by_bytes_at_both_ends() {
        for accepted in ["a", &"n".repeat(MAX_NAME_BYTES)] {
            SecretName::parse(accepted).expect("within bounds");
        }
        for refused in ["", &"n".repeat(MAX_NAME_BYTES + 1)] {
            SecretName::parse(refused).expect_err("outside bounds");
        }
    }

    #[test]
    fn a_multibyte_name_is_measured_in_bytes_like_the_zig_bound() {
        // Sixty-four characters that are not sixty-four bytes. Counting
        // characters would accept a name the other daemon refuses, and the two
        // must agree on the same row.
        let multibyte = "é".repeat(MAX_NAME_BYTES);
        assert_eq!(multibyte.len(), MAX_NAME_BYTES * 2);
        SecretName::parse(&multibyte)
            .expect_err("sixty-four characters, one hundred and twenty-eight bytes");
    }

    #[test]
    fn only_a_non_empty_object_becomes_a_body() {
        SecretBody::parse(&data(r#"{"api_key":"sk-live"}"#)).expect("one key is enough");

        for refused in [
            "{}",
            "[]",
            r#"["provider","anthropic"]"#,
            r#""a string""#,
            "42",
            "null",
            "true",
        ] {
            assert!(
                SecretBody::parse(&data(refused)).is_err(),
                "{refused} is not a non-empty object"
            );
        }
    }

    #[test]
    fn a_positional_array_is_refused_where_a_derived_struct_would_take_it() {
        // The hole `afd_core::json` exists to close, closed here by the same
        // move: the body is requested as a MAP, so `serde_json` accepts `{`
        // alone and a two-element array cannot fill a two-key credential.
        SecretBody::parse(&data(r#"["anthropic","sk-live"]"#))
            .expect_err("a two-element array cannot fill a two-key credential");
    }

    #[test]
    fn the_bound_is_measured_on_the_canonical_form_not_the_request() {
        // Whitespace the caller sent is not stored, so it must not count. This
        // body is far over the bound as written and well under it canonically.
        let padded = format!(r#"{{ "k"{} : "v" }}"#, " ".repeat(MAX_DATA_BYTES));
        SecretBody::parse(&data(&padded)).expect("the canonical form is seven bytes");
    }

    #[test]
    fn a_canonical_form_past_the_bound_is_refused() {
        let oversized = format!(r#"{{"k":"{}"}}"#, "v".repeat(MAX_DATA_BYTES));
        let refused = SecretBody::parse(&data(&oversized))
            .expect_err("a body past the bound is not storable");

        assert_eq!(refused.code(), afd_core::error_code::VAULT_DATA_TOO_LARGE);
    }

    #[test]
    fn an_empty_object_and_a_non_object_answer_the_same_code() {
        // Both are `UZ-VAULT-001`: the caller's remedy is the same sentence,
        // and a client branching on the difference could not act on it.
        for refused in ["{}", r#""a string""#] {
            let error = SecretBody::parse(&data(refused)).expect_err("refused");
            assert_eq!(error.code(), afd_core::error_code::VAULT_DATA_INVALID);
        }
    }

    #[test]
    fn the_projection_comes_from_the_same_bytes_that_will_be_sealed() {
        // The property the whole type exists for: there is no way to hold a
        // body whose projection describes a different object, because the
        // constructor is the only producer of either.
        let body = SecretBody::parse(&data(r#"{"provider":"anthropic","api_key":"sk-live"}"#))
            .expect("a well-formed credential");

        assert_eq!(body.projection().kind, Kind::ProviderKey);
        assert!(body.projection().has_key);

        let sealed: serde_json::Value = serde_json::from_slice(body.plaintext()).unwrap();
        assert_eq!(
            sealed.get("provider").and_then(serde_json::Value::as_str),
            Some("anthropic")
        );
    }

    #[test]
    fn the_canonical_bytes_keep_the_order_the_caller_sent() {
        // `preserve_order` is what makes this crate and the Zig daemon
        // stringify one body to the same bytes. A sorted map would still be
        // correct JSON and would silently break that agreement.
        let body = SecretBody::parse(&data(r#"{"z":"1","a":"2"}"#)).expect("a valid body");

        assert_eq!(body.plaintext(), br#"{"z":"1","a":"2"}"#);
    }
}
