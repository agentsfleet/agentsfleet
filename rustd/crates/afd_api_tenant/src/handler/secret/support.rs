//! The shapes and refusals the secret verbs share.
//!
//! Split from `secret.rs` for the file-length cap. These three are the parts no
//! single verb owns: a refusal sentence two deletes could raise, the projection
//! the list emits, and the body reader both writing verbs go through so their
//! refusals are one sentence rather than two spellings of it.

use std::borrow::Cow;

use afd_auth::{Principal, Scope, require_scope};
use afd_connector::Provider;
use afd_vault::{SecretName, SecretSummary};
use axum::body::Bytes;

use afd_http::handler::Refusable;

use crate::handler::Refusal;

use super::{DETAIL_BODY_REQUIRED, DETAIL_MALFORMED_JSON, EVENT_DELETE, STATE_REFERENCED};

/// The sentence a still-referenced delete is refused with.
///
/// The plural is spelled out because a dashboard shows this string to the
/// operator who has to go and remove those entries.
pub(super) fn referenced_detail(entries: u32) -> String {
    let plural = if entries == 1 { "y" } else { "ies" };
    format!("Secret is referenced by {entries} model registry entr{plural}")
}

/// One stored projection, as the list emits it.
pub(super) fn summary(held: &SecretSummary) -> afd_api_wire::secret::SecretSummary<'_> {
    afd_api_wire::secret::SecretSummary {
        name: Cow::Borrowed(&held.name),
        created_at: held.created_at,
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
    afd_http::handler::read_body::<T>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))
}

/// The connector whose connection `name` holds, if it is a grant key.
pub(super) fn connection_named(name: &SecretName) -> Option<Provider> {
    Provider::parse(name.as_str()).filter(|provider| provider.grant_key() == name.as_str())
}

/// What deleting a connection by its secret name needs beside `secret:write`.
///
/// That delete IS the connector's Disconnect, so it asks what Disconnect's own
/// route row asks; otherwise a key scoped to secrets could unplug a connector.
const DISCONNECT: &[Scope] = &[Scope::ConnectorWrite];

/// The guard's own sentence for that requirement, so both refusals read alike.
pub(super) const DETAIL_DISCONNECT_SCOPE: &str = "Requires scope connector:write";

/// Refuses a caller who may write secrets but may not disconnect a connector.
pub(super) fn may_disconnect(principal: &Principal) -> Result<(), Refusal> {
    require_scope(principal, DISCONNECT)
        .map_err(|denied| Refusal::coded(denied.code(), DETAIL_DISCONNECT_SCOPE))
}

/// The refusal a delete answers with, on either path.
///
/// The one refusal whose sentence the call site knows better than the error's
/// own table does: `detail()` is a `&'static str` and the count is not, so the
/// counted form is rendered here, the same split `Refusal::preconditioned`
/// makes for a stale `ETag`.
pub(super) fn delete_refusal<E: Refusable>(referenced: Option<u32>, refused: E) -> Refusal {
    match referenced {
        Some(entries) => {
            Refusal::conflict_detailed(EVENT_DELETE, referenced_detail(entries), STATE_REFERENCED)(
                refused,
            )
        }
        None => Refusal::at(EVENT_DELETE)(refused),
    }
}

#[cfg(test)]
mod tests {
    use super::referenced_detail;

    /// The refusal sentence agrees with itself about how many entries there are.
    ///
    /// An operator reads this string in a dashboard and then goes and removes
    /// those entries, so "1 entries" is not a cosmetic problem — it is the one
    /// number in the sentence, spelled wrong. Both arms are held because the
    /// singular is the one a plural-by-default `format!` gets wrong.
    #[test]
    fn the_referenced_refusal_agrees_with_its_own_count() {
        assert_eq!(
            referenced_detail(1),
            "Secret is referenced by 1 model registry entry"
        );
        assert_eq!(
            referenced_detail(2),
            "Secret is referenced by 2 model registry entries"
        );
        assert_eq!(
            referenced_detail(0),
            "Secret is referenced by 0 model registry entries"
        );
    }
}
