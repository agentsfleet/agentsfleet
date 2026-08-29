//! The workspace's fleets over HTTP: the list, and the install.
//!
//! The port of `http/handlers/fleets/list.zig` and `create.zig`. The item half —
//! read, edit, purge — is [`detail`], split along the line the route table
//! already draws: everything here is addressed by a workspace alone, everything
//! there by a fleet as well.
//!
//! # Ownership is not checked here, and that is the point
//!
//! Every Zig handler under `fleets/` opens with a hand-written
//! `authorizeWorkspace` call, and one that forgot would be a cross-tenant read
//! with nothing failing. Here the check is a LAYER mounted from the route's own
//! template, so [`WorkspaceContext`] is a handler saying which workspace it is
//! acting in — never a handler deciding whether it may.

pub mod detail;
pub mod memory;
mod memory_request;
pub mod message;

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_core::paging::Cursor;
use afd_fleet_lifecycle::{After, FleetPage, FleetRow, Install, Installed, LibrarySource};
use afd_fleet_runtime::FleetName;
use afd_wire::fleet::{
    FleetSummary, FleetsResponse, InstallFleetRequest, InstalledFleetResponse, Triggers, WebhookUrl,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;
use serde_json::value::RawValue;

use crate::auth::WorkspaceContext;
use crate::handler::{Refusal, parameter};
use crate::services::{Services, WorkspaceFleets as _};

/// The scoped events each verb's failures are logged under.
const EVENT_LIST: &str = "fleet_list_failed";
const EVENT_INSTALL: &str = "fleet_install_failed";

/// The page a caller naming no `limit` gets.
///
/// Twenty where the workspace directory serves fifty — each is its own Zig
/// handler's number, and parity keeps them apart.
const LIST_LIMIT_DEFAULT: u32 = 20;

/// The most rows one list page may carry.
const LIST_LIMIT_MAX: u32 = 100;

/// The pre-guideline cursor spelling, refused outright.
///
/// Not ignored: a caller still sending it would silently read page one forever,
/// so the rename is reported rather than absorbed.
const QUERY_CURSOR_RETIRED: &str = "cursor";

/// The `starting_after` parameter's name.
const QUERY_STARTING_AFTER: &str = "starting_after";

/// The `limit` parameter's name.
const QUERY_LIMIT: &str = "limit";

/// The refusal a caller still paging with `cursor` earns.
pub const DETAIL_RETIRED_CURSOR: &str = "cursor is retired on this list; page with starting_after";

/// The refusal a `starting_after` this daemon never issued earns.
pub const DETAIL_INVALID_CURSOR: &str = "Invalid cursor format";

/// The refusal an install body this daemon cannot read earns.
pub const DETAIL_MALFORMED_JSON: &str = "Request body is not valid JSON";

/// The refusal an install naming no library entry earns.
pub const DETAIL_LIBRARY_REQUIRED: &str =
    "install requires platform_library_id or tenant_library_id";

/// The refusal an install naming both tiers earns.
pub const DETAIL_LIBRARY_AMBIGUOUS: &str =
    "install accepts exactly one of platform_library_id or tenant_library_id";

/// The refusal a name override this daemon will not store earns.
pub const DETAIL_NAME_INVALID: &str = "name is required (max 64 chars, slug-safe)";

/// The refusal a tenant library id that is not an identifier earns.
pub const DETAIL_TENANT_LIBRARY_ID: &str = "tenant_library_id must be a valid UUIDv7";

/// The body an empty POST reads as — `req.body() orelse "{}"`, ported.
const EMPTY_OBJECT: &[u8] = b"{}";

/// `GET /v1/workspaces/{workspace_id}/fleets` — one page, newest first.
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let query = query.unwrap_or_default();
    if parameter(&query, QUERY_CURSOR_RETIRED).is_some() {
        return Err(Refusal::malformed(DETAIL_RETIRED_CURSOR));
    }
    let limit = limit_or_default(parameter(&query, QUERY_LIMIT));
    let after = parse_cursor(parameter(&query, QUERY_STARTING_AFTER))?;

    let page = services
        .fleets()
        .page(&owned.workspace, after.as_ref(), limit)
        .await
        .map_err(Refusal::at(EVENT_LIST))?;
    Ok(Json(page_response(&page)).into_response())
}

/// `POST /v1/workspaces/{workspace_id}/fleets` — install one.
pub(crate) async fn install<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    body: Bytes,
) -> Result<Response, Refusal> {
    let body = if body.is_empty() { EMPTY_OBJECT } else { &body };
    let request = afd_core::json::object_from_slice::<InstallFleetRequest<'_>>(body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;

    let source = library_source(&request)?;
    let name = request
        .name
        .as_deref()
        .map(FleetName::parse)
        .transpose()
        .map_err(|_unusable| Refusal::malformed(DETAIL_NAME_INVALID))?;

    let installed = services
        .fleets()
        .install(&owned.workspace, &Install { source, name }, services.now())
        .await
        .map_err(Refusal::at(EVENT_INSTALL))?;
    Ok((
        StatusCode::CREATED,
        Json(installed_response(&installed, services.deployment())),
    )
        .into_response())
}

/// Which library tier this install draws from, or the refusal it earns.
///
/// The neither-set and both-set cases are two different sentences, which is why
/// the wire struct carries two optional fields rather than an untagged enum: a
/// parse failure could not tell a caller which of the two they did.
fn library_source<'a>(request: &'a InstallFleetRequest<'a>) -> Result<LibrarySource<'a>, Refusal> {
    match (
        request.platform_library_id.as_deref(),
        request.tenant_library_id.as_deref(),
    ) {
        (Some(_platform), Some(_tenant)) => Err(Refusal::malformed(DETAIL_LIBRARY_AMBIGUOUS)),
        (Some(platform), None) => Ok(LibrarySource::Platform(platform)),
        (None, Some(tenant)) => Uuid7::parse(tenant)
            .map(LibrarySource::Tenant)
            .map_err(|_not_an_identifier| Refusal::malformed(DETAIL_TENANT_LIBRARY_ID)),
        (None, None) => Err(Refusal::malformed(DETAIL_LIBRARY_REQUIRED)),
    }
}

/// The page size asked for, clamped, or the default.
///
/// Total rather than fallible, and that is `list.zig`'s behaviour kept: a limit
/// that will not parse reads as the default here, where the workspace directory
/// answers a 400. Each is its own handler's vocabulary — the divergence would be
/// making them agree, and a client sitting on either would change class.
fn limit_or_default(raw: Option<&str>) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok())
        .filter(|asked| *asked > 0)
        .map_or(LIST_LIMIT_DEFAULT, |asked| asked.min(LIST_LIMIT_MAX))
}

/// The decoded boundary, or the refusal a foreign token earns.
fn parse_cursor(raw: Option<&str>) -> Result<Option<After>, Refusal> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let cursor =
        Cursor::parse(raw).map_err(|_foreign| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    let Cursor::Timestamp { at_ms, id } = cursor else {
        // Some other list's token: the fleets walk keys on `(created_at, id)`,
        // and a text-boundary cursor names a sort this one does not have.
        return Err(Refusal::malformed(DETAIL_INVALID_CURSOR));
    };
    let id = Uuid7::parse(&id).map_err(|_not_a_fleet| Refusal::malformed(DETAIL_INVALID_CURSOR))?;
    Ok(Some(After {
        created_at_ms: at_ms,
        id,
    }))
}

/// One page, and the cursor that continues it.
///
/// The cursor is emitted only when a row EXISTS beyond this page — decided by
/// over-fetching rather than by the page being full — so a client never spends a
/// request on a page that comes back empty.
fn page_response(page: &FleetPage) -> FleetsResponse<'_> {
    let next_cursor = page.more.then(|| page.rows.last()).flatten().map(|last| {
        Cow::Owned(
            Cursor::Timestamp {
                at_ms: last.created_at_ms,
                id: last.id.clone(),
            }
            .to_string(),
        )
    });
    FleetsResponse {
        items: page.rows.iter().map(summary).collect(),
        total: page.rows.len(),
        next_cursor,
    }
}

/// One row as the wire shows it.
fn summary(row: &FleetRow) -> FleetSummary<'_> {
    FleetSummary {
        id: Cow::Borrowed(&row.id),
        name: Cow::Borrowed(&row.name),
        status: Cow::Borrowed(row.status.as_str()),
        created_at: row.created_at_ms,
        updated_at: row.updated_at_ms,
        triggers: triggers(row.triggers.as_ref()),
        events_processed: row.events_processed,
        budget_used_nanos: row.budget_used_nanos,
    }
}

/// The stored trigger projection, spliced as raw JSON.
///
/// Text that will not parse renders as `null`, which is
/// `parseFromSlice(…) catch null` kept exactly: a legacy row with a malformed
/// projection shows no triggers rather than failing the whole page.
pub(super) fn triggers(stored: Option<&afd_fleet_lifecycle::Triggers>) -> Triggers {
    stored.and_then(|projection| RawValue::from_string(projection.as_json_text().to_owned()).ok())
}

/// The install's reply, with the webhook addresses this deployment answers on.
fn installed_response<'a>(
    installed: &'a Installed,
    deployment: &'a str,
) -> InstalledFleetResponse<'a> {
    InstalledFleetResponse {
        fleet_id: Cow::Borrowed(installed.id.as_str()),
        name: Cow::Borrowed(&installed.name),
        status: Cow::Borrowed(installed.status.as_str()),
        webhook_urls: installed
            .webhook_sources
            .iter()
            .map(|source| WebhookUrl {
                source: Cow::Borrowed(source),
                url: Cow::Owned(format!(
                    "{deployment}/v1/webhooks/{}/{source}",
                    installed.id.as_str()
                )),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::indexing_slicing,
        clippy::panic,
        reason = "fixed test fixtures fail loudly when their contract changes"
    )]

    use super::*;
    use afd_fleet_lifecycle::FleetStatus;

    fn fleet_id() -> Uuid7 {
        Uuid7::parse("0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010")
            .unwrap_or_else(|error| panic!("fixture id is canonical: {error}"))
    }

    #[test]
    fn a_page_cursor_names_the_last_returned_row() {
        let id = fleet_id().to_string();
        let page = FleetPage {
            rows: vec![FleetRow {
                id: id.clone(),
                name: "reviewer".to_owned(),
                status: FleetStatus::Active,
                created_at_ms: 42,
                updated_at_ms: 43,
                triggers: None,
                events_processed: 7,
                budget_used_nanos: 11,
            }],
            more: true,
        };

        let response = page_response(&page);

        assert_eq!(response.total, 1);
        assert_eq!(response.items[0].id, id);
        let parsed = parse_cursor(response.next_cursor.as_deref())
            .unwrap_or_else(|error| panic!("emitted cursor parses: {error:?}"))
            .unwrap_or_else(|| panic!("a page with more rows emits a cursor"));
        assert_eq!(parsed.created_at_ms, 42);
        assert_eq!(parsed.id.as_str(), id);
    }

    #[test]
    fn an_install_reply_builds_each_webhook_from_the_deployment() {
        let installed = Installed {
            id: fleet_id(),
            name: "reviewer".to_owned(),
            status: FleetStatus::Active,
            webhook_sources: vec!["github".into(), "slack".into()],
        };

        let response = installed_response(&installed, "https://api.example.test");

        assert_eq!(response.fleet_id, installed.id.as_str());
        assert_eq!(response.webhook_urls.len(), 2);
        assert_eq!(
            response.webhook_urls[0].url,
            format!(
                "https://api.example.test/v1/webhooks/{}/github",
                installed.id.as_str()
            )
        );
    }
}
