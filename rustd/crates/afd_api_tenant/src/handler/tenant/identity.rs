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

use afd_auth::principal::{Person, PersonCredential};
use afd_wire::identity::{CurrentUserResponse, credential_class};
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TerminalCredentials as _};

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
        .cli_credentials()
        .user_of(person.subject().as_str())
        .await
        .map_err(Refusal::at(EVENT_PROFILE))?;

    Ok(Json(CurrentUserResponse {
        user_id: Cow::Borrowed(profile.id.as_str()),
        email: Cow::Borrowed(&profile.email),
        display_name: profile.display_name.as_deref().map(Cow::Borrowed),
        tenant_id: Cow::Borrowed(profile.tenant.as_str()),
        tenant_name: Cow::Borrowed(&profile.tenant_name),
        credential: Cow::Borrowed(class_of(person.credential())),
        scopes: scopes_of(person),
    })
    .into_response())
}

/// What this caller may do, in the spelling scopes are claimed under.
///
/// Beside [`class_of`] because the two are the response's only DERIVED fields —
/// everything else is a column — and both are worth a test that does not need a
/// datastore to reach them.
fn scopes_of(person: &Person) -> Vec<Cow<'static, str>> {
    person
        .scopes()
        .iter()
        .map(|scope| Cow::Borrowed(scope.wire()))
        .collect()
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
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use super::{class_of, credential_class, scopes_of};
    use afd_auth::principal::{Person, PersonCredential, Subject};
    use afd_auth::scope::{Scope, ScopeSet};
    use afd_core::id::Uuid7;

    /// A fixture identifier, well formed and otherwise meaningless.
    const TENANT: &str = "0193c5e0-0000-7000-8000-000000001234";

    /// A person holding exactly `scopes`.
    fn person_holding(scopes: ScopeSet) -> Person {
        Person::new(
            PersonCredential::CliCredential,
            Uuid7::parse(TENANT).expect("the fixture identifier is well formed"),
            Subject::new("user_fixture").expect("the fixture subject is not blank"),
            scopes,
        )
    }

    /// Capabilities render in the spelling a claim carries them in.
    ///
    /// Against `Scope::wire` rather than a literal, because that spelling is what
    /// a claim is parsed from and what a client branches on — two ends of one
    /// fact, and re-spelling it here is how the two drift (RULE UFS).
    #[test]
    fn scopes_render_in_the_wire_spelling() {
        let held = ScopeSet::from_scopes(&[Scope::FleetRead, Scope::SecretRead]);

        let rendered = scopes_of(&person_holding(held));

        assert!(
            rendered
                .iter()
                .any(|scope| scope == Scope::FleetRead.wire())
        );
        assert!(
            rendered
                .iter()
                .any(|scope| scope == Scope::SecretRead.wire())
        );
        assert_eq!(rendered.len(), 2, "no scope is rendered twice");
    }

    /// A person who holds nothing renders an empty list, never a missing key.
    ///
    /// The distinction a reader depends on: "you hold no capabilities" and "this
    /// answer does not say" are different facts, and an omitted key collapses
    /// them.
    #[test]
    fn a_person_holding_nothing_renders_an_empty_list() {
        assert!(scopes_of(&person_holding(ScopeSet::from_scopes(&[]))).is_empty());
    }

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

    /// This handler emits no log line of its own, so none can carry a person.
    ///
    /// A source assertion, because that is where the invariant lives. The
    /// refusal path logs through [`Refusal::at`], which carries an error code, a
    /// request identifier and an event name and has no field a profile could
    /// reach; the success path logs nothing at all. What would break the rule is
    /// somebody adding `tracing::info!(email = …)` here, and this is what fails
    /// when they do — a runtime capture could not, because the fields it would
    /// look for do not exist yet.
    #[test]
    fn the_identity_handler_logs_nothing_itself() {
        let source = include_str!("identity.rs");
        // Everything before the test module: the tests below legitimately name
        // these things while asserting about them.
        let production = source
            .split_once("#[cfg(test)]")
            .map_or(source, |(before, _tests)| before);

        for emitter in ["tracing::", "println!", "eprintln!", "dbg!"] {
            assert!(
                !production.contains(emitter),
                "the identity read must emit no `{emitter}` of its own: a log here \
                 is one field away from publishing an address to an operator's \
                 aggregator"
            );
        }
    }

    /// A session narrowed to one workspace is still a session.
    ///
    /// The field names the CLASS, not its reach. A `==` against a value rather
    /// than a match on the variant would answer the wrong word here, which is
    /// the same trap `admits` in `afd_http::auth::person` documents.
    #[test]
    fn a_workspace_scoped_session_still_renders_as_a_session() {
        let ceiling = PersonCredential::SessionToken {
            workspace_scope: Some(
                afd_core::id::Uuid7::parse("0193c5e0-0000-7000-8000-000000001234")
                    .expect("the fixture identifier is well formed"),
            ),
        };

        assert_eq!(class_of(&ceiling), credential_class::SESSION_TOKEN);
    }
}
