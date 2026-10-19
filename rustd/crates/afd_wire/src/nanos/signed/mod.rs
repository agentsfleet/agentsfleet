//! A signed amount on the wire, for the one nanos field that can fall below
//! zero: a tenant's balance, which a settlement runs past zero into a debt.
//!
//! The same decimal string as [`super`], with a leading minus allowed on a
//! non-zero amount. Zero has one spelling: an `i64` holds no negative zero, so
//! `"-0"` is never written, and [`PATTERN`] refuses it for a client that
//! checks. Every other amount stays on [`super::PATTERN`].

use serde::Serializer;

/// The canonical signed spelling, as the `OpenAPI` document publishes it.
pub const PATTERN: &str = "^(0|-?[1-9][0-9]*)$";

/// Writes `nanos` as a decimal string, signed when it is below zero.
///
/// # Errors
/// Whatever the serializer reports; formatting itself cannot fail.
pub fn serialize<S: Serializer>(nanos: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    super::serialize(nanos, serializer)
}

/// The published shape of a signed nanos field: a string matching [`PATTERN`].
#[cfg(feature = "openapi")]
#[derive(Debug, Clone, Copy)]
pub struct SignedDecimal;

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for SignedDecimal {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .pattern(Some(PATTERN))
            .description(Some(
                "A whole number of nanos (10⁻⁹ US dollars) as a decimal string, \
                 negative for a debt.",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for SignedDecimal {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("SignedNanos")
    }
}

#[cfg(test)]
mod tests;
