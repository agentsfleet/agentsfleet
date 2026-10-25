//! The request-header shapes more than one route reads.
//!
//! Same reason as [`super::path`]: a parameter's name is a literal token the
//! macro will not take a `const` for, so a header read by several routes was
//! spelled at each of them. The field identifier carries the declaration and
//! `rename` carries the wire spelling, once.

use utoipa::IntoParams;

/// The strong entity tag a conditional write is checked against.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Header)]
pub struct IfMatch {
    /// Current `ETag` from the resource's GET response. An outdated value returns 412 with
    /// the current `etag`; omitting it returns 428.
    #[param(rename = "If-Match")]
    pub if_match: String,
}

/// The replay key a creation is accepted once under.
#[derive(Debug, IntoParams)]
#[into_params(parameter_in = Header)]
pub struct IdempotencyKey {
    /// `UUIDv7` key. The same key and body replay the accepted result; a
    /// changed body under the same key conflicts.
    #[param(rename = "Idempotency-Key")]
    pub idempotency_key: String,
}
