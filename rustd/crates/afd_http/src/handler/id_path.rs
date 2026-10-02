//! One identifier path segment, parsed before the handler runs.
//!
//! A route naming one resource by `UUIDv7` can take this in place of
//! `Path<String>` and a `map_err` in its body. The parse is the extractor's,
//! and the sentence is the one fact a route supplies, through the marker type
//! it names. The team routes under `/v1/tenants/me/invites` and
//! `/v1/tenants/me/members`, and the invite accept, take it. Older routes,
//! such as the API key and command-line credential verbs, still parse their
//! segment by hand.

use std::marker::PhantomData;

use afd_core::id::Uuid7;
use axum::extract::{FromRequestParts, Path};
use axum::response::{IntoResponse, Response};
use http::request::Parts;

use super::parse_id;

/// What a route's identifier segment is, told by the refusal a malformed one
/// earns.
pub trait IdSegment: Send + Sync + 'static {
    /// The refusal a segment that is not a `UUIDv7` earns.
    const DETAIL: &'static str;
}

/// A route's one identifier segment, already a `UUIDv7`.
///
/// Named before anything that reads a datastore in a handler's arguments, so a
/// malformed identifier is refused before the request costs a statement.
#[derive(Debug)]
pub struct IdPath<S>(Uuid7, PhantomData<S>);

impl<S> IdPath<S> {
    /// The identifier the segment named.
    #[must_use]
    pub const fn id(&self) -> &Uuid7 {
        &self.0
    }
}

impl<St: Send + Sync, S: IdSegment> FromRequestParts<St> for IdPath<S> {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &St) -> Result<Self, Response> {
        let Path(raw) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        parse_id(&raw, S::DETAIL)
            .map(|id| Self(id, PhantomData))
            .map_err(IntoResponse::into_response)
    }
}

#[cfg(test)]
#[path = "id_path/tests.rs"]
mod tests;
