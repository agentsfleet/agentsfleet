//! Nanos on the wire: a decimal string of whole nanos, never a JSON number.
//!
//! Every nanos amount (a balance, a charge, a rate, a cost) is an `i64` in
//! Rust and a string such as `"3000000"` on the wire, carrying `#[serde(with =
//! "afd_wire::nanos")]` (or [`option`] for an optional one). A JSON number is
//! read by JavaScript as a double, which rounds every integer past 2⁵³; a
//! string keeps every nano, the mapping protobuf's JSON form gives every
//! 64-bit integer. The `OpenAPI` document declares the same shape once, as the
//! `Nanos` component `Decimal` publishes under the `openapi` feature.
//!
//! A request's string is refused rather than coerced unless it is the
//! canonical spelling [`PATTERN`] names: digits only, no sign, no leading
//! zero, no whitespace, and no larger than a `BIGINT` column holds. A served
//! amount comes from a column the schema keeps non-negative, so it always
//! matches the pattern too.

use std::borrow::Cow;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

/// The canonical spelling, as the `OpenAPI` document publishes it.
pub const PATTERN: &str = "^(0|[1-9][0-9]*)$";

/// The refusal for a string that is not the canonical spelling.
pub const NOT_CANONICAL: &str =
    "nanos must be a decimal string of whole nanos: digits only, no sign, no leading zero";

/// The refusal for an amount larger than a 64-bit signed column holds.
pub const TOO_LARGE: &str = "nanos must not exceed 9223372036854775807";

/// Writes `nanos` as a decimal string.
///
/// # Errors
/// Whatever the serializer reports; formatting itself cannot fail.
pub fn serialize<S: Serializer>(nanos: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(nanos)
}

/// Reads a canonical decimal string of whole nanos.
///
/// # Errors
/// [`NOT_CANONICAL`] or [`TOO_LARGE`].
pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let text = Cow::<'de, str>::deserialize(deserializer)?;
    parse(&text).map_err(D::Error::custom)
}

/// The amount `text` spells, refused unless it is canonical.
///
/// What a query parameter naming an amount parses through, so a query and a
/// body refuse the same strings.
///
/// # Errors
/// [`NOT_CANONICAL`] or [`TOO_LARGE`].
pub fn parse(text: &str) -> Result<i64, &'static str> {
    let canonical = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if !canonical {
        return Err(NOT_CANONICAL);
    }
    text.parse().map_err(|_overflow| TOO_LARGE)
}

/// The published shape of every nanos field: a string matching [`PATTERN`].
///
/// A field names it through `#[schema(value_type = afd_wire::nanos::Decimal)]`
/// (or `Option<Decimal>`), so the document declares the shape once as the
/// `Nanos` component and every field keeps its own description beside the
/// reference.
#[cfg(feature = "openapi")]
#[derive(Debug, Clone, Copy)]
pub struct Decimal;

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for Decimal {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .pattern(Some(PATTERN))
            .description(Some(
                "A whole number of nanos (10⁻⁹ US dollars) as a decimal string.",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for Decimal {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("Nanos")
    }
}

/// The same rules for an optional amount; `null` is no amount.
pub mod option {
    use std::borrow::Cow;

    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Writes `Some` as [`super::serialize`] does, `None` as `null`.
    ///
    /// # Errors
    /// Whatever the serializer reports.
    pub fn serialize<S: Serializer>(nanos: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error> {
        match nanos {
            Some(nanos) => super::serialize(nanos, serializer),
            None => serializer.serialize_none(),
        }
    }

    /// Reads `null` as `None` and anything else as [`super::deserialize`] does.
    ///
    /// # Errors
    /// As [`super::deserialize`].
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<i64>, D::Error> {
        Option::<Cow<'de, str>>::deserialize(deserializer)?
            .map(|text| super::parse(&text).map_err(D::Error::custom))
            .transpose()
    }
}

#[cfg(test)]
mod tests;
