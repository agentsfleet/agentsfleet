//! `/v1/connectors/{provider}/callback` — one path, two routes, two guards.
//!
//! `GET` is where the PROVIDER sends the browser, so it can carry no credential
//! of ours and is unauthenticated by necessity: it reads nothing, writes
//! nothing, and redirects to the dashboard's own relay. `POST` is the dashboard
//! coming back with the person's bearer, and it is the only endpoint in this
//! family that redeems a code or writes a connection.
//!
//! # Why the workspace check is in the handler and not in a layer
//!
//! Every other workspace-scoped verb names its workspace in the PATH, so
//! `Ownership::of(template)` mounts the check in front of the handler and a
//! handler cannot forget it. This template names no workspace: the workspace is
//! inside the signed state, which cannot be read until the signature has been
//! checked. So the check runs here, and it runs at the one point in the order
//! where it is both possible and still early enough — see below.
//!
//! # The order is the security property, and the types hold it
//!
//! Verify the state, check it against the person presenting it, re-authorise
//! the workspace, and only THEN consume the nonce. Consuming first would let
//! any authenticated person burn somebody else's in-flight connect by replaying
//! its callback URL. `afd_connector` makes that ordering a type — [`Spent`] is
//! constructible only by the spend, and the finish takes one — so this handler
//! could not reorder it if it tried.

use std::sync::Arc;

use afd_connector::{Finishing, Handoff, Landed, Provider, Rejected, callback};
use afd_core::error_code;
use afd_core::id::Uuid7;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::{StatusCode, header};

use super::{EVENT_WRITE, provider_of, relay_uri, state_secret, unconfigured};
use crate::auth::{Acting, PersonIdentity};
use crate::handler::{BrokenEscape, Refusal, decoded_parameter};
use crate::services::{Services, WorkspaceConnectors as _, WorkspaceOwnership as _};

/// The scoped event a refused callback is logged under.
const EVENT_REFUSED: &str = "connector_callback_refused";

/// The scoped event a failed ownership check is logged under.
const EVENT_OWNERSHIP: &str = "connector_callback_ownership_failed";

/// Query parameters a provider hands back, named once each (RULE UFS).
const PARAM_CODE: &str = "code";
/// See [`PARAM_CODE`].
const PARAM_STATE: &str = "state";
/// See [`PARAM_CODE`].
const PARAM_LOCATION: &str = "location";
/// See [`PARAM_CODE`].
const PARAM_INSTALLATION_ID: &str = "installation_id";

/// The refusal a callback carrying no state earns.
///
/// `callback.zig`'s `S_MISSING_STATE`. Without one there is nothing to verify
/// and nothing to identify the round-trip by, so it is refused before any store
/// is asked.
const DETAIL_MISSING_STATE: &str = "Missing state";

/// The refusal a completion carrying no authorization code earns.
const DETAIL_MISSING_CODE: &str = "Missing code";

/// The refusal a query this daemon cannot decode earns.
const DETAIL_BAD_QUERY: &str = "Bad query string";

/// The refusal a state that did not survive its checks earns.
///
/// `callback.zig`'s `S_STATE_INVALID`. ONE sentence for forged, expired, spent
/// and foreign, matching the single registry code: a caller able to tell them
/// apart learns which check they got past, and every one has the same remedy.
const DETAIL_STATE_INVALID: &str = "Invalid or expired connect state";

/// The refusal a caller who does not hold the state's workspace earns.
const DETAIL_FOREIGN_WORKSPACE: &str = "Workspace access denied";

/// The word a spent or expired single-use slot is logged under.
///
/// Not a [`Rejected`] variant, because it is not a verify outcome: the state
/// was genuine, and this round-trip had already been completed or had timed
/// out. The caller is told the same code either way — both mean start the
/// connect again — and this is what tells the two apart in an operator's log.
const REASON_SLOT_SPENT: &str = "state_slot_spent";

/// `GET /v1/connectors/{provider}/callback` — the browser, arriving.
///
/// Redirects to the dashboard's relay carrying what the provider sent. It reads
/// no state binding, consumes no nonce and touches no store, which is what
/// makes an unauthenticated route safe to serve: everything it does with the
/// parameters is hand them onward, and the dashboard then returns with a bearer
/// through [`complete`].
///
/// # Errors
/// `UZ-CONN-004` for a provider this daemon does not ship, `UZ-REQ-001` for a
/// callback carrying no state or a query this daemon cannot decode, and
/// `UZ-CONN-001` for a dashboard base that is not a URL.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/connectors/{provider}/callback",
    tag = afd_http::openapi::tag::CONNECTORS,
    operation_id = "connector_callback",
    summary = "Relay a provider callback to the dashboard",
    description = concat!(
        "Compatibility URL for provider registrations that still target the ",
        "API host. It forwards the browser to the dashboard callback with a ",
        "fixed legacy marker. The authenticated endpoint uses that marker ",
        "only to echo the old redirect URL during token exchange. This ",
        "endpoint never exchanges a provider code or changes connector data. ",
        "New provider registrations use ",
        "`https://<APP_HOST>/api/connectors/{provider}/callback` for the ",
        "matching environment. The dashboard posts the current Bearer token ",
        "to the authenticated callback completion method. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 302, description = afd_http::openapi::FOUND),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn relay<D: Services>(
    State(services): State<Arc<D>>,
    Path(provider_segment): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let provider = provider_of(&provider_segment)?;
    let query = query.unwrap_or_default();

    let state = optional(&query, PARAM_STATE)?
        .ok_or_else(|| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_MISSING_STATE))?;
    let code = optional(&query, PARAM_CODE)?;
    let location = optional(&query, PARAM_LOCATION)?;
    let installation_id = optional(&query, PARAM_INSTALLATION_ID)?;

    let destination = callback::relay_url(
        services.dashboard(),
        provider,
        Handoff {
            code: code.as_deref(),
            state: &state,
            location: location.as_deref(),
            installation_id: installation_id.as_deref(),
        },
    )
    .ok_or_else(unconfigured)?;

    Ok(found(&destination))
}

/// `POST /v1/connectors/{provider}/callback` — the dashboard, completing.
///
/// # Errors
/// `UZ-CONN-004` for an unshipped provider, `UZ-REQ-001` for a missing state or
/// code, `UZ-CONN-002` for a state that is forged, expired, spent or somebody
/// else's, `UZ-AUTH-003` for a caller who does not hold the workspace the state
/// names, `UZ-CONN-001` for a provider this deployment configured no app for,
/// and the vendor and datastore failures the exchange can raise.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/connectors/{provider}/callback",
    tag = afd_http::openapi::tag::CONNECTORS,
    operation_id = "connector_callback_complete",
    summary = "Complete a provider connection",
    description = concat!(
        "The dashboard calls this endpoint after the provider returns to the ",
        "browser. The caller needs `connector:write` and must be the same ",
        "person who started the signed connection state. The signed state ",
        "binds the workspace, a keyed tag of the starter identity, a nonce, ",
        "and expiry. The endpoint verifies identity and workspace access ",
        "before consuming the nonce or exchanging a provider code. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 302, description = afd_http::openapi::FOUND),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn complete<D: Services>(
    State(services): State<Arc<D>>,
    Acting(principal): Acting,
    person: PersonIdentity,
    Path(provider_segment): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let provider = provider_of(&provider_segment)?;
    let query = query.unwrap_or_default();

    let presented = optional(&query, PARAM_STATE)?
        .ok_or_else(|| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_MISSING_STATE))?;
    let code = optional(&query, PARAM_CODE)?
        .ok_or_else(|| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_MISSING_CODE))?;
    let location = optional(&query, PARAM_LOCATION)?;

    let Some(admin) = services.platform_admin_workspace() else {
        return Err(unconfigured());
    };
    let secret = state_secret(&services).await?;
    let redirect_uri = relay_uri(&services, provider)?;

    // Step 1 — the signature, the window, and whether this is the person who
    // started it. Touches no store, which is what keeps a replayed callback
    // costing a hash rather than a round trip.
    let verified = services
        .connectors()
        .verify(
            provider,
            &secret,
            &presented,
            person.subject(),
            services.now(),
        )
        .map_err(|rejected| state_refused(provider, rejected.reason()))?;

    // Step 2 — the workspace, re-authorised BEFORE the nonce is spent. A state
    // this daemon signed carrying an unparseable workspace is this build's
    // fault, so it refuses as an invalid state rather than telling a person
    // their workspace is not theirs.
    let workspace = Uuid7::parse(verified.workspace())
        .map_err(|_unparseable| state_refused(provider, Rejected::Malformed.reason()))?;
    let owned = services
        .workspaces()
        .authorize(&principal, &workspace)
        .await
        .map_err(Refusal::at(EVENT_OWNERSHIP))?;
    if owned.is_none() {
        return Err(Refusal::coded(
            error_code::AUTH_FORBIDDEN,
            DETAIL_FOREIGN_WORKSPACE,
        ));
    }

    // Step 3 — the single-use slot, spent last and exactly once. A slot already
    // spent or expired answers exactly as a forged state does: both mean start
    // the connect again.
    let Some(spent) = services
        .connectors()
        .spend(provider, &verified)
        .await
        .map_err(Refusal::at(EVENT_WRITE))?
    else {
        return Err(state_refused(provider, REASON_SLOT_SPENT));
    };

    let landed = services
        .connectors()
        .finish(
            Finishing {
                admin,
                provider,
                spent: &spent,
                code: &code,
                location: location.as_deref(),
                redirect_uri: &redirect_uri,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    match landed {
        Landed::Connected => Ok(connected(&services, &workspace)),
        Landed::NotConfigured => Err(unconfigured()),
    }
}

/// One optional query parameter, decoded.
///
/// Every value here is the provider's, not this daemon's: an authorization code
/// is opaque and may carry any byte a vendor chose, so it decodes rather than
/// being read raw — see [`crate::handler::decoded_parameter`].
///
/// # Errors
/// `UZ-REQ-001` for a broken percent-escape.
fn optional(query: &str, name: &str) -> Result<Option<String>, Refusal> {
    decoded_parameter(query, name)
        .map(|value| value.map(std::borrow::Cow::into_owned))
        .map_err(|BrokenEscape| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_BAD_QUERY))
}

/// The refusal a state that did not survive its checks earns.
///
/// The REASON goes to the operator and the code to the caller. That split is
/// the point: the word names which check failed, which is what an operator
/// debugging a connect needs, and telling the caller would hand an attacker a
/// probe for how far a forged state got.
fn state_refused(provider: Provider, reason: &'static str) -> Refusal {
    // Hoisted for the reason every other call-bearing log field in this crate
    // is: the `log` bridge duplicates the expression and llvm-cov scores the
    // dead copy.
    let provider_field = provider.id();
    tracing::debug!(provider = provider_field, reason, event = EVENT_REFUSED);
    Refusal::coded(error_code::CONNECTOR_STATE_INVALID, DETAIL_STATE_INVALID)
}

/// Where a person lands once the connect has finished.
///
/// A 200 when the destination cannot be built, and deliberately not a 500: the
/// grant IS sealed and the connection IS live by this point, so failing the
/// request would tell a person their connect did not work when it did — and the
/// next thing they would do is press Connect again. `callback.zig` reaches the
/// same conclusion for the same reason.
fn connected<D: Services>(services: &Arc<D>, workspace: &Uuid7) -> Response {
    callback::connected_url(services.dashboard(), workspace).map_or_else(
        || StatusCode::OK.into_response(),
        |destination| found(&destination),
    )
}

/// A redirect to `destination`.
///
/// 302 rather than 303: the daemon this ports answers 302 on both callback
/// legs, and a browser follows either with a GET here because both arrive at a
/// destination that only serves one.
fn found(destination: &str) -> Response {
    // A URL this daemon composed through `url`, so every byte is already in the
    // header's alphabet. An unparseable value would be a bug in that composer
    // rather than anything the caller sent, and answering 200 is what the
    // person needs either way — the connect itself is unaffected.
    header::HeaderValue::from_str(destination).map_or_else(
        |_unrenderable| StatusCode::OK.into_response(),
        |location| (StatusCode::FOUND, [(header::LOCATION, location)]).into_response(),
    )
}
