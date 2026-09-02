//! The two verbs over a person's own `afc_` credentials.
//!
//! # Who may call them, and why a scope could not have said so
//!
//! A credential names a PERSON, so these routes admit only the classes that are
//! one. Minting is narrower still and takes a browser session alone, so a
//! credential cannot mint its own successor. Both rules live in the extractor a
//! handler names — [`FreshSession`] for the mint, [`HumanIdentity`] for the
//! revoke — rather than in a check either body performs, so a handler cannot
//! reach its service having skipped one.
//!
//! A required scope could not express either rule. A tenant api-key carries the
//! whole tenant grant, so it already holds every scope this family might ask
//! for; principal MODE is the only thing separating an organisation from a
//! human, and the route table says so by asking for no scope at all.
//!
//! Nothing here reads credential material back. The mint response is the only
//! place a raw value appears, and it exists once.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_tenant::cli_credential::{MachineName, MintRequest, Revealed};
use afd_wire::tenant::{MintCliCredentialRequest, MintedCliCredentialResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, StatusCode, header};

use crate::auth::{FreshSession, HumanIdentity};
use crate::client::Origin;
use crate::handler::Refusal;
use crate::services::{Services, TerminalCredentials as _};

/// The scoped events each verb's failures are logged under.
const EVENT_MINT: &str = "cli_credential_mint_failed";
const EVENT_REVOKE: &str = "cli_credential_revoke_failed";
const EVENT_SUBJECT: &str = "cli_credential_subject_unresolved";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MINT_BODY: &str = "Malformed JSON body";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_CREDENTIAL_ID: &str = "id must be a valid UUIDv7";

/// `POST /v1/cli-credentials` — mint this machine's credential, revealing it once.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/cli-credentials",
    tag = afd_http::openapi::tag::CLI_CREDENTIALS,
    operation_id = "create_cli_credential",
    summary = "Mint a command-line credential",
    description = concat!(
        "Creates a durable credential for one machine, with the `afc_` ",
        "prefix. The raw credential is returned once. Only its SHA-256 hash ",
        "is saved, so it cannot be read back later. A credential belongs to a ",
        "person, not to a team. Call this with a browser session token or ",
        "with an existing command-line credential. A tenant API key is ",
        "refused. Minting revokes whatever credential the same machine name ",
        "already held. One machine holds one live credential at a time. ",
    ),
    request_body = MintCliCredentialRequest,
    params(
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = MintedCliCredentialResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn mint<D: Services>(
    State(services): State<Arc<D>>,
    identity: FreshSession,
    origin: Origin,
    body: Bytes,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let request = afd_core::json::object_from_slice::<MintCliCredentialRequest<'_>>(&body)
        .map_err(|_unreadable| Refusal::malformed(DETAIL_MINT_BODY))?;
    let machine = MachineName::parse(&request.machine_name).map_err(Refusal::at(EVENT_MINT))?;

    let user = services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_SUBJECT))?;

    // The deployment is the one ANSWERING this request, never a value the
    // caller supplied: a credential and the deployment that minted it are one
    // fact, and a client-asserted host would let them disagree.
    let mint = MintRequest {
        user: &user.id,
        tenant: &user.tenant,
        machine,
        deployment: services.deployment(),
        from_address: origin.address.as_str(),
    };

    let revealed = services
        .cli_credentials()
        .mint(&mint, services.now())
        .await
        .map_err(Refusal::at(EVENT_MINT))?;
    Ok(revealed_response(&revealed))
}

/// `DELETE /v1/cli-credentials/{id}` — revoke one of this user's credentials.
///
/// Scoped to the owner in the statement itself, so a guessed identifier
/// belonging to somebody else revokes nothing and reads as not found.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/cli-credentials/{id}",
    tag = afd_http::openapi::tag::CLI_CREDENTIALS,
    operation_id = "revoke_cli_credential",
    summary = "Revoke a command-line credential",
    description = concat!(
        "Retires one of your credentials. The terminal holding it is refused ",
        "on its next call, with the code `UZ-AUTH-023`. Running `agentsfleet ",
        "logout` calls this for the credential on that machine. Browser ",
        "sessions are untouched, so signing out of a terminal does not sign ",
        "you out of the dashboard. ",
    ),
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn revoke<D: Services>(
    State(services): State<Arc<D>>,
    identity: HumanIdentity,
    Path(credential_id): Path<String>,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let credential = Uuid7::parse(&credential_id)
        .map_err(|_unparseable| Refusal::malformed(DETAIL_CREDENTIAL_ID))?;

    let user = services
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_SUBJECT))?;

    services
        .cli_credentials()
        .revoke(&user.id, &credential, services.now())
        .await
        .map_err(Refusal::at(EVENT_REVOKE))?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The mint reply, with the header that keeps it out of a cache.
///
/// `no-store` and not merely `no-cache`, for [`super::api_key`]'s reason: the
/// body carries a credential in plaintext exactly once, and an intermediary
/// holding a copy is a copy nobody can revoke.
fn revealed_response(revealed: &Revealed) -> Response {
    (
        StatusCode::CREATED,
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, max-age=0"),
        )],
        Json(MintedCliCredentialResponse {
            id: Cow::Borrowed(revealed.id.as_str()),
            credential: Cow::Borrowed(revealed.credential.expose()),
            machine_name: Cow::Borrowed(&revealed.machine_name),
            deployment: Cow::Borrowed(&revealed.deployment),
        }),
    )
        .into_response()
}
