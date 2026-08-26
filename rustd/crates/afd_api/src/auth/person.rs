//! The proven person, and the narrower one the dashboard surface needs.
//!
//! Sibling of [`super::identity`], which does the same job for the runner
//! plane. The split is the one [`crate::route`] already draws: a runner speaks
//! for itself, a person acts through one of three credential classes, and one
//! route family cares WHICH of the three.
//!
//! # Why there are two extractors and not one with a flag
//!
//! Approving and cancelling a login are dashboard actions. `docs/AUTH.md` keeps
//! the classes distinct after authentication for exactly this: an `agt_t`
//! tenant api-key resolves to the same person with the same capabilities as
//! their browser session, and it must still not be able to approve a device
//! login — the whole point of the flow is that a human looked at a screen.
//!
//! A boolean parameter would put that rule in every handler body. A second
//! TYPE puts it in the signature, so a handler that takes
//! [`DashboardIdentity`] cannot run for an api-key and there is no arm where
//! it might.

use afd_auth::principal::{Person, PersonCredential, Principal};
use afd_core::error_code::{self, ErrorCode};
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse as _, Response};
use http::request::Parts;

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The refusal a handler mounted without its guard answers with.
const DETAIL_UNPROVEN: &str = "person identity required";

/// The refusal a credential class that cannot approve a login answers with.
///
/// Says nothing about which class WOULD work. A caller holding an api-key
/// learns that this endpoint is not for api-keys from the documentation, not
/// from an error message enumerating what else it might try.
const DETAIL_NOT_DASHBOARD: &str = "Clerk user context missing";

/// A person, whatever they proved it with.
#[derive(Debug, Clone)]
pub struct PersonIdentity(pub Person);

impl<S: Send + Sync> FromRequestParts<S> for PersonIdentity {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(person(parts).map(Self).ok_or_else(unproven))
    }
}

/// A person acting through a browser session, which is the only class that
/// proves a human is looking at a screen right now.
#[derive(Debug, Clone)]
pub struct DashboardIdentity(pub Person);

impl DashboardIdentity {
    /// The identity-provider subject, which is what a session is owned BY.
    #[must_use]
    pub fn subject(&self) -> &str {
        self.0.subject().as_str()
    }
}

impl<S: Send + Sync> FromRequestParts<S> for DashboardIdentity {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(match person(parts) {
            // A 401 rather than a 403 for the wrong class, matching the Zig
            // refusal: the caller has not proven the thing this route needs
            // proven, and a capability they could be granted would not change
            // it.
            Some(person) => match person.credential() {
                PersonCredential::SessionToken { .. } => Ok(Self(person)),
                PersonCredential::TenantApiKey | PersonCredential::CliCredential => Err(refuse(
                    error_code::AUTH_UNAUTHORIZED,
                    DETAIL_NOT_DASHBOARD,
                    "session_credential_required",
                )),
            },
            None => Err(unproven()),
        })
    }
}

/// The person the guard layer proved, if it ran at all.
///
/// An `Option` rather than a `Result<_, Response>`: an `axum::Response` is over
/// a hundred bytes, and a helper returning one in its error position makes
/// every caller's `Result` that size (`clippy::result_large_err`). The absence
/// has exactly one meaning, so a type carrying a whole response to say it would
/// be carrying it to be discarded.
fn person(parts: &Parts) -> Option<Person> {
    parts
        .extensions
        .get::<Principal>()
        .and_then(Principal::person)
        .cloned()
}

/// The refusal for a handler that ran without its guard.
///
/// Unreachable through this crate's router for the reason
/// [`super::identity`]'s twin is: a bearer route is mounted only through the
/// helper that applies the guard with the route's own metadata. It exists for a
/// handler somebody mounts by hand, and it answers 500 because the caller's
/// credential was never judged — telling them it is bad would be a wiring bug
/// wearing an authentication error's clothes.
fn unproven() -> Response {
    refuse(
        error_code::INTERNAL_OPERATION_FAILED,
        DETAIL_UNPROVEN,
        "person_identity_unproven",
    )
}

/// Writes a refusal from an extractor, which has no service to log through.
fn refuse(code: ErrorCode, detail: &'static str, event: &'static str) -> Response {
    let request_id = RequestId::mint();
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let code_field = code.as_str();
    let request_id_field = request_id.as_str();
    tracing::debug!(
        error_code = code_field,
        request_id = request_id_field,
        event,
    );
    ProblemResponse::new(code, detail, request_id).into_response()
}
