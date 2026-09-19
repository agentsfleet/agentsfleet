//! `GET /v1/users/me` — the one read that answers who the caller is.
//!
//! # Who may call it, and why no scope says so
//!
//! Every person credential may, and that is the whole rule: the extractor is
//! [`PersonIdentity`], the widest of the four class policies, so a browser
//! session, a tenant api-key and a command-line credential all reach this
//! handler and a runner token does not. The route table asks for no capability,
//! because "may you read your own name" is not a question a capability can be
//! short for — and a scope here would break the client that uses this route to
//! prove a fresh login, which is any terminal, capability or none.
//!
//! An `agt_t` key is deliberately admitted rather than refused. It resolves to
//! the person who created it, so answering with that person is the honest
//! reply; the `credential` field says which class asked, so a caller can tell a
//! terminal's answer from an automation's.
//!
//! # Nothing here is echoed
//!
//! The request carries no body, no query and no path parameter. Every field in
//! the reply comes from the row the proven subject names or from the principal
//! the guard built, so there is no input to validate and no field a caller can
//! steer.

use std::borrow::Cow;
use std::sync::Arc;

use afd_auth::principal::PersonCredential;
use afd_wire::identity::{CurrentUserResponse, credential_class};
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{CallerProfiles as _, Services};

/// The scoped event this read's failures are logged under.
const EVENT_PROFILE: &str = "caller_profile_unresolved";

/// `GET /v1/users/me` — the person, the tenant, and how they proved it.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/users/me",
    tag = afd_http::openapi::tag::USERS,
    operation_id = "get_current_user",
    summary = "Read the calling person",
    description = concat!(
        "Returns the person this credential belongs to, the tenant they act ",
        "in, which credential class proved them, and what they may do. Call ",
        "it with a browser session token, a tenant API key, or a ",
        "command-line credential. A tenant API key answers with the person ",
        "who created it. No capability is required, so this is also the ",
        "cheapest way to confirm that a credential still authenticates. ",
    ),
    params(
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = CurrentUserResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::UNKNOWN_SUBJECT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn current<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let profile = services
        .profiles()
        .profile(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PROFILE))?;

    Ok(Json(CurrentUserResponse {
        user_id: Cow::Borrowed(profile.user.as_str()),
        email: Cow::Borrowed(&profile.email),
        display_name: profile.display_name.as_deref().map(Cow::Borrowed),
        tenant_id: Cow::Borrowed(profile.tenant.as_str()),
        tenant_name: Cow::Borrowed(&profile.tenant_name),
        credential: Cow::Borrowed(class_of(person.credential())),
        scopes: person
            .scopes()
            .iter()
            .map(|scope| Cow::Borrowed(scope.wire()))
            .collect(),
    })
    .into_response())
}

/// The wire word for the class that proved this caller.
///
/// Total over [`PersonCredential`], so a fourth credential class does not
/// compile until somebody has said what this route calls it. A session token's
/// workspace ceiling is ignored on purpose: a session narrowed to one workspace
/// is still a session, and the field names the CLASS rather than its reach.
const fn class_of(credential: &PersonCredential) -> &'static str {
    match credential {
        PersonCredential::SessionToken { .. } => credential_class::SESSION_TOKEN,
        PersonCredential::TenantApiKey => credential_class::TENANT_API_KEY,
        PersonCredential::CliCredential => credential_class::CLI_CREDENTIAL,
    }
}

#[cfg(test)]
mod tests {
    use super::{class_of, credential_class};
    use afd_auth::principal::PersonCredential;

    /// Each class renders its own wire word, taken from the shared constants.
    ///
    /// Inline rather than in the router suite because the harness store sits
    /// over a pool with no Postgres, so no test out there can reach a 200 to
    /// read the field off. These are also the words a client branches on, which
    /// is why they are asserted against `afd_wire`'s constants rather than
    /// re-spelled here where the two could drift apart (RULE UFS).
    #[test]
    fn every_credential_class_renders_its_own_wire_word() {
        assert_eq!(
            class_of(&PersonCredential::TenantApiKey),
            credential_class::TENANT_API_KEY
        );
        assert_eq!(
            class_of(&PersonCredential::CliCredential),
            credential_class::CLI_CREDENTIAL
        );
        assert_eq!(
            class_of(&PersonCredential::SessionToken {
                workspace_scope: None
            }),
            credential_class::SESSION_TOKEN
        );
    }

    /// A session narrowed to one workspace is still a session.
    ///
    /// The field names the CLASS, not its reach. A `==` against a value rather
    /// than a match on the variant would answer the wrong word here, which is
    /// the same trap `admits` in `afd_http::auth::person` documents.
    #[test]
    fn a_workspace_scoped_session_still_renders_as_a_session() {
        let ceiling = PersonCredential::SessionToken {
            workspace_scope: Some(afd_core::id::Uuid7::parse(
                "0193c5e0-0000-7000-8000-000000001234",
            )
            .expect("the fixture identifier is well formed")),
        };

        assert_eq!(class_of(&ceiling), credential_class::SESSION_TOKEN);
    }
}
