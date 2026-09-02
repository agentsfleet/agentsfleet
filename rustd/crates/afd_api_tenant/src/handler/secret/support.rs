//! The shapes and refusals the secret verbs share.
//!
//! Split from `secret.rs` for the file-length cap. These three are the parts no
//! single verb owns: a refusal sentence two deletes could raise, the projection
//! the list emits, and the body reader both writing verbs go through so their
//! refusals are one sentence rather than two spellings of it.

use std::borrow::Cow;

use afd_vault::SecretSummary;
use axum::body::Bytes;

use crate::handler::Refusal;

use super::{DETAIL_BODY_REQUIRED, DETAIL_MALFORMED_JSON};

/// The sentence a still-referenced delete is refused with.
///
/// `secrets.zig`'s wording, plural included, because a dashboard shows this
/// string to the operator who has to go and remove those entries.
pub(super) fn referenced_detail(entries: u32) -> String {
    let plural = if entries == 1 { "y" } else { "ies" };
    format!("Secret is referenced by {entries} model registry entr{plural}")
}

/// One stored projection, as the list emits it.
pub(super) fn summary(held: &SecretSummary) -> afd_wire::secret::SecretSummary<'_> {
    afd_wire::secret::SecretSummary {
        name: Cow::Borrowed(&held.name),
        created_at: held.created_at_ms,
        kind: held.kind().as_str(),
        provider: held.provider().map(Cow::Borrowed),
        base_url: held.base_url().map(Cow::Borrowed),
    }
}

/// Reads a request body as `T`, refusing anything that is not an object.
///
/// Shared by both writing verbs so their two refusals are one sentence. An
/// EMPTY body is told apart from a malformed one: the fleet install can default
/// to `{}` because every field there is optional, and here there would be no
/// secret to store.
pub(super) fn read_body<'b, T: serde::Deserialize<'b>>(body: &'b Bytes) -> Result<T, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    afd_core::json::object_from_slice::<T>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))
}
