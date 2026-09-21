//! `DELETE /v1/workspaces/{workspace_id}/library-entries/{entry_id}` — taking
//! one entry back out.
//!
//! The verb slot 460 withheld. Its header argued an onboarded entry would be
//! "retired by visibility"; `schema/917` supersedes that, and
//! `docs/architecture/fleet_bundles.md` records why.
//!
//! # Nothing downstream notices
//!
//! A fleet installed from an entry copied the bundle out of it at install time
//! and carries its own; no foreign key points here from the fleet side. So
//! removal is permanent and complete — no soft-delete column, no marker row, no
//! visibility flip — and a re-onboard of the same bytes mints a new entry
//! rather than converging on a tombstone.

use std::sync::Arc;

use afd_core::id::Uuid7;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde::Deserialize;

use super::{DETAIL_ENTRY_ID, EVENT_REMOVE, EVENT_REMOVED};
use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::Services;

/// The segments the item template carries.
///
/// A named struct rather than `Path<String>`, for the reason
/// [`crate::handler::secret::SecretPath`] already records: the template
/// carries TWO parameters, and `Path<String>` deserializes a single one. It
/// fails in the extractor and answers 500 before the handler body runs — so
/// the verb is not merely wrong on an edge, it never works at all.
#[derive(Debug, Deserialize)]
pub(crate) struct EntryPath {
    /// The entry named in the path, still text: it is parsed below, where a
    /// failure becomes the refusal a caller can act on.
    pub(crate) entry_id: String,
}

/// `DELETE /v1/workspaces/{workspace_id}/library-entries/{entry_id}`.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/library-entries/{entry_id}",
    tag = afd_http::openapi::tag::FLEET_LIBRARY,
    operation_id = "delete_workspace_library_entry",
    summary = "Remove a Fleet library entry this workspace onboarded",
    description = concat!(
        "Idempotent. Removing an id already gone still returns 204. So does ",
        "one naming another workspace's entry. The two are indistinguishable ",
        "on purpose. Every statement here is scoped by workspace. Separating ",
        "the two would need an unscoped read, and its only effect would be to ",
        "confirm the id exists somewhere. Removal is permanent. It cannot ",
        "disturb a fleet installed from the entry: that fleet copied the ",
        "bundle at install time and carries its own. Re-onboarding the same ",
        "bytes afterwards mints a new entry with a new id. Distinct from ",
        "`delete_platform_fleet_library`, which is operator-held. ",
    ),
    params(
        afd_http::openapi::path::Workspace,
        ("entry_id" = String, Path, description = "The entry's UUIDv7, from `list_workspace_library_entries`."),
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(EntryPath { entry_id }): Path<EntryPath>,
) -> Result<Response, Refusal> {
    let entry = parse_entry_id(&entry_id)?;

    let removed = services
        .libraries()
        .remove_entry(&owned.workspace, &entry)
        .await
        .map_err(Refusal::at(EVENT_REMOVE))?;

    // The line carries the outcome and nothing of the bundle: no markdown, no
    // manifest, no content hash, no source reference. The identifiers name
    // rows, and a row id is not a secret — what a bundle HOLDS is.
    let workspace_id = owned.workspace.as_str();
    let library_entry_id = entry.as_str();
    let event = EVENT_REMOVED;
    tracing::info!(workspace_id, library_entry_id, removed, event);

    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The entry a path segment names.
///
/// `Refusal::malformed` rather than a minted code, which is what every other
/// path identifier in this daemon does.
fn parse_entry_id(raw: &str) -> Result<Uuid7, Refusal> {
    Uuid7::parse(raw).map_err(|_not_an_identifier| Refusal::malformed(DETAIL_ENTRY_ID))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking on an unmet precondition"
    )]

    use super::parse_entry_id;

    /// A path segment that is not an identifier is refused before any
    /// statement, and without minting a registry code.
    ///
    /// The absence of the code is the assertion, not decoration: a family code
    /// promises a caller a documented recovery, and "you typed a UUID wrong"
    /// has none beyond typing it again.
    #[test]
    fn a_malformed_entry_id_is_refused_without_a_registry_code() {
        for raw in [
            "not-a-uuid",
            "",
            "0195b4ba8d3a7f138abccd0000000002",
            "../fleets",
        ] {
            let refusal = parse_entry_id(raw)
                .expect_err("a malformed path segment must not parse as an entry id");
            let rendered = format!("{refusal:?}");
            assert!(
                !rendered.contains("UZ-"),
                "a malformed path segment minted a registry code: {rendered}"
            );
        }
    }

    /// A well-formed identifier parses, so the refusal above is about the
    /// shape rather than about the function refusing everything.
    #[test]
    fn a_uuidv7_parses() {
        parse_entry_id("0195b4ba-8d3a-7f13-8abc-cd0000000002")
            .expect("a UUIDv7 path segment names an entry");
    }
}
