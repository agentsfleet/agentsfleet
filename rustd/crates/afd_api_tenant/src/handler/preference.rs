//! The workspace's dashboard preferences over HTTP, and the onboarding
//! checklist derived from them.
//!
//! The port of `workspaces/preferences.zig` and `workspaces/onboarding.zig`.
//! Three verbs across three templates, and every one of them answers with a
//! whole bag or a whole checklist — there is no read of a single key, because
//! the dashboard holds this state in one piece and a fragment would make it
//! merge on the client.
//!
//! # An unreadable bag must look like an empty one
//!
//! This surface fails open TOWARD showing onboarding. A person whose
//! preferences cannot be read is a person who has not dismissed the checklist,
//! so a stored value that will not parse is DROPPED from the bag rather than
//! failing the read. A row only lands through the write path below, which
//! proves the value parses, so a malformed one is corruption rather than
//! client input — and hiding the checklist from somebody because one row rotted
//! is the one outcome this endpoint must not produce.
//!
//! # The subject is not the user id
//!
//! The principal carries the identity provider's subject; every preference row
//! keys on the `core.users.id` it maps to. That mapping is a READ, and a
//! subject with no row is refused rather than bootstrapped — inventing a user
//! here would fork identity ownership away from the signup path that owns it.

use std::sync::Arc;

use afd_core::error_code;
use afd_tenant::preference::{MAX_PREF_VALUE_BYTES, Pref, PrefKey, bag_is_true};
use afd_wire::preference::{OnboardingResponse, PreferencesResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use serde::Deserialize;
use serde_json::value::RawValue;

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspacePreferences as _};

/// The scoped events each verb's failures are logged under.
const EVENT_READ: &str = "preferences_read_failed";
const EVENT_WRITE: &str = "preference_write_failed";
const EVENT_ONBOARDING: &str = "onboarding_read_failed";

/// The refusal a subject with no `core.users` row earns.
///
/// `S_USER_CONTEXT_REQUIRED`, kept verbatim — a dashboard shows this to
/// somebody whose sign-up has not finished landing.
const DETAIL_USER_CONTEXT: &str = "User context required";

/// The refusal a path key outside the registry earns.
const DETAIL_KEY_UNKNOWN: &str = "pref_key is not a known preference";

/// The refusal an oversize value earns.
const DETAIL_VALUE_TOO_LARGE: &str = "pref value exceeds the 1 KiB limit";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED_JSON: &str = "Malformed JSON";

/// The refusal a write with no body earns.
const DETAIL_BODY_REQUIRED: &str = "Request body required";

/// The segment the item template carries beside the workspace.
///
/// A named struct rather than `Path<String>`: the template carries TWO
/// parameters, and a single-field extraction would fail before the handler ran.
#[derive(Debug, Deserialize)]
pub(crate) struct PreferencePath {
    /// The preference key named in the path, still text.
    pub pref_key: String,
}

/// `GET /v1/workspaces/{workspace_id}/preferences` — this person's whole bag.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/preferences",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "get_workspace_preferences",
    summary = "Read the caller's dashboard preferences for a workspace",
    description = concat!(
        "Returns every dashboard preference the calling user has set in this ",
        "workspace, as an object keyed by preference key. A user who has set ",
        "nothing gets `{\"prefs\": {}}` - never a 404, so a client can tell \"no ",
        "preferences\" from \"preferences unavailable\" without branching. ",
        "Preferences are per user AND per workspace: a second workspace ",
        "starts its onboarding checklist fresh. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = OnboardingResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
) -> Result<Response, Refusal> {
    let user = resolve_user(&services, person.subject(), EVENT_READ).await?;
    let bag = services
        .preferences()
        .bag(&user, &owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    Ok(respond_with_bag(&bag))
}

/// `PUT /v1/workspaces/{workspace_id}/preferences/{pref_key}` — write one key.
///
/// The key is refused at the PATH and the value at the BODY, which is the
/// split the two registry codes describe: `UZ-PREFS-001` says "that is not a
/// preference", `UZ-PREFS-002` says "that is too much of one".
///
/// Answers with the whole bag, not the written key — see the module note.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/v1/workspaces/{workspace_id}/preferences/{pref_key}",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "put_workspace_preference",
    summary = "Write one dashboard preference",
    description = concat!(
        "Upserts a single preference for the calling user in this workspace ",
        "and returns the full updated bag. The request body IS the value - ",
        "any well-formed JavaScript Object Notation (JSON) value up to 1 KiB, ",
        "stored verbatim and never interpreted by the server. `pref_key` must ",
        "be one the dashboard declares; anything else is refused with `UZ- ",
        "PREFS-001` and no row is written. Concurrent writes to one key are ",
        "last-write-wins by design: a preference is a single toggle, so a ",
        "lost write costs one click. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn write<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(PreferencePath { pref_key }): Path<PreferencePath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let key = PrefKey::parse(&pref_key)
        .ok_or_else(|| Refusal::coded(error_code::PREF_KEY_UNKNOWN, DETAIL_KEY_UNKNOWN))?;
    let value = read_value(&body)?;

    let user = resolve_user(&services, person.subject(), EVENT_WRITE).await?;
    services
        .preferences()
        .upsert(&user, &owned.workspace, key, value, services.now())
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    let bag = services
        .preferences()
        .bag(&user, &owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    Ok(respond_with_bag(&bag))
}

/// `GET /v1/workspaces/{workspace_id}/onboarding` — the whole checklist.
///
/// Five derivable signals from one round trip, folded with three preference
/// keys read on the same store. One HTTP call, one authorization, where the
/// dashboard used to make six.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/onboarding",
    tag = afd_http::openapi::tag::WORKSPACES,
    operation_id = "get_workspace_onboarding",
    summary = "Read the workspace's onboarding checklist state",
    description = concat!(
        "Returns every signal the Getting Started checklist needs in one ",
        "call. Five signals are derived server-side in a single query. A ",
        "model is configured, a fleet exists, a credential exists, an event ",
        "has been processed, and a steer event exists. Three more are the ",
        "caller's stored UI preferences: dismissed, collapsed, and CLI ",
        "ticked. This replaces six separate requests with one. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn onboarding<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
) -> Result<Response, Refusal> {
    let signals = services
        .preferences()
        .signals(&owned.workspace, &owned.tenant)
        .await
        .map_err(Refusal::at(EVENT_ONBOARDING))?;

    let user = resolve_user(&services, person.subject(), EVENT_ONBOARDING).await?;
    let bag = services
        .preferences()
        .bag(&user, &owned.workspace)
        .await
        .map_err(Refusal::at(EVENT_ONBOARDING))?;

    Ok(Json(OnboardingResponse {
        model_configured: signals.model_configured,
        has_fleet: signals.has_fleet,
        has_secret: signals.has_secret,
        has_processed_event: signals.has_processed_event,
        has_steer_event: signals.has_steer_event,
        cli_ticked: bag_is_true(&bag, PrefKey::GettingStartedCliTicked),
        dismissed: bag_is_true(&bag, PrefKey::GettingStartedDismissed),
        collapsed: bag_is_true(&bag, PrefKey::GettingStartedCollapsed),
    })
    .into_response())
}

/// The internal user id behind a proven subject, or the refusal for none.
async fn resolve_user<D: Services>(
    services: &Arc<D>,
    subject: &str,
    event: &'static str,
) -> Result<String, Refusal> {
    services
        .preferences()
        .resolve_user(subject)
        .await
        .map_err(Refusal::at(event))?
        .ok_or_else(|| Refusal::forbidden(DETAIL_USER_CONTEXT))
}

/// Renders a bag, dropping any row whose stored text will not parse.
///
/// See the module note on why a rotted row is dropped rather than raised.
fn respond_with_bag(bag: &[Pref]) -> Response {
    Json(PreferencesResponse {
        prefs: bag
            .iter()
            .filter_map(|pref| {
                let value = serde_json::from_str::<&RawValue>(&pref.value).ok()?;
                Some((pref.key.as_str(), value))
            })
            .collect(),
    })
    .into_response()
}

/// Reads a write body as the opaque JSON text it will be stored as.
///
/// Bounded BEFORE it is parsed: the cap exists so an unbounded blob cannot
/// become free tenant storage, and parsing a megabyte to then refuse it would
/// spend exactly what the cap is there to save.
fn read_value(body: &Bytes) -> Result<&str, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    if body.len() > MAX_PREF_VALUE_BYTES {
        return Err(Refusal::coded(
            error_code::PREF_VALUE_TOO_LARGE,
            DETAIL_VALUE_TOO_LARGE,
        ));
    }
    // Parsed only to refuse malformed input at the boundary. The TEXT is what
    // is stored, so a value always round-trips byte for byte.
    let text =
        std::str::from_utf8(body).map_err(|_invalid| Refusal::malformed(DETAIL_MALFORMED_JSON))?;
    serde_json::from_str::<&RawValue>(text)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED_JSON))?;
    Ok(text)
}
