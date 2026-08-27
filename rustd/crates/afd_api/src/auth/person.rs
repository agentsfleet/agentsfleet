//! The proven person, narrowed to the credential classes a route admits.
//!
//! Sibling of [`super::identity`], which does the same job for the runner
//! plane. The split is the one [`crate::route`] already draws: a runner speaks
//! for itself, a person acts through one of three credential classes, and some
//! route families care WHICH of the three.
//!
//! # Why the rule is a type and not a flag
//!
//! `docs/AUTH.md` keeps the classes distinct after authentication for a reason
//! a capability check cannot express: an `agt_t` tenant api-key resolves to the
//! same person with the same scopes as their browser session. It is exactly as
//! capable and must still not approve a device login, because the whole point
//! of that flow is that a human looked at a screen. No required scope can say
//! that; only the class can.
//!
//! A boolean parameter would put the rule in every handler body, where the
//! fourth handler forgets it. A TYPE puts it in the signature: a handler taking
//! [`Dashboard`] cannot run for an api-key, and there is no arm where it might.
//!
//! # Why one generic and not four types
//!
//! There were four hand-written extractors, each about thirty-five lines, and
//! each differing from its siblings in exactly two ways: which classes it
//! admits, and what it says when it refuses. Everything else — read the
//! principal, match the class, mint a request id, log, render problem+json —
//! was copied. Four copies of a security check is four places for the fifth
//! rule somebody adds to be subtly different from the other four.
//!
//! Here the copied part exists once, in one [`FromRequestParts`] implementation
//! generic over [`ClassPolicy`], and each rule is DATA: a list of admitted
//! classes, a code, a sentence, an event. That makes the rules enumerable,
//! which is what lets `person_policy.rs` assert the whole matrix — every
//! policy against every class — instead of testing whichever handlers somebody
//! remembered to cover.

use std::marker::PhantomData;

use afd_auth::principal::{Person, PersonCredential, Principal};
use afd_core::error_code::{self, ErrorCode};
use axum::extract::FromRequestParts;
use axum::response::{IntoResponse as _, Response};
use http::request::Parts;

use crate::envelope::ProblemResponse;
use crate::request_id::RequestId;

/// The refusal a handler mounted without its guard answers with.
const DETAIL_UNPROVEN: &str = "person identity required";

/// The refusal a class that cannot approve a login answers with.
///
/// Says nothing about which class WOULD work. A caller holding an api-key
/// learns that this endpoint is not for api-keys from the documentation, not
/// from an error message enumerating what else it might try.
pub const DETAIL_NOT_DASHBOARD: &str = "Clerk user context missing";

/// The refusal a credential that is not a person's earns on the command-line
/// credential surface.
///
/// A tenant api-key is what this refuses. It resolves to a person and carries
/// that person's capabilities, so no required SCOPE could express the rule — a
/// tenant key already holds every scope this family might name. Principal MODE
/// is the only thing separating an organisation from a human.
pub const DETAIL_PERSON_REQUIRED: &str =
    "A command-line credential belongs to a person; a tenant API key cannot manage one";

/// The refusal minting earns from anything but a browser sign-in.
///
/// Distinct from [`DETAIL_PERSON_REQUIRED`] deliberately, and the distinction
/// is load-bearing. Both refusals answer `UZ-AUTH-001`, so a test asserting the
/// CODE cannot tell "not a browser sign-in" from "not a person" — it passes
/// either way, including when the freshness rule is gone and an `afc_`
/// credential is quietly minting its own successors. The sentence is what pins
/// the rule uniquely, which is why it is `pub` and why a test must never
/// re-spell it (RULE UFS).
pub const DETAIL_SESSION_REQUIRED: &str = "Minting a command-line credential requires a browser sign-in; an existing credential cannot mint another";

/// One route family's rule about which credential classes may act.
///
/// Implemented by the zero-sized markers below, never by a caller: the point is
/// that the set of rules is closed and enumerable, so [`Self::ALL`]-style
/// matrix tests can cover every one.
pub trait ClassPolicy: Send + Sync + 'static {
    /// The classes this policy lets through. Everything else is refused.
    const ADMITS: &'static [PersonCredential];
    /// The registry code the refusal answers with.
    const CODE: ErrorCode;
    /// The sentence the refusal carries.
    const DETAIL: &'static str;
    /// The scoped event the refusal is logged under.
    const EVENT: &'static str;
}

/// Every class. "Some human is behind this request", and nothing narrower.
#[derive(Debug, Clone, Copy)]
pub struct AnyClass;

impl ClassPolicy for AnyClass {
    const ADMITS: &'static [PersonCredential] = &[
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        PersonCredential::TenantApiKey,
        PersonCredential::CliCredential,
    ];
    // Unreachable: nothing is refused. Stated anyway because the trait says
    // every policy has a refusal, and a policy that admits everything today
    // needs one the day somebody narrows it.
    const CODE: ErrorCode = error_code::AUTH_FORBIDDEN;
    const DETAIL: &'static str = DETAIL_PERSON_REQUIRED;
    const EVENT: &'static str = "person_class_refused";
}

/// A browser session only — the class that proves a human is at a screen NOW.
///
/// Approving a device login takes this, because a stolen `agt_t` key resolving
/// to the same person must not be able to approve its own logins forever.
#[derive(Debug, Clone, Copy)]
pub struct DashboardClass;

impl ClassPolicy for DashboardClass {
    const ADMITS: &'static [PersonCredential] = &[PersonCredential::SessionToken {
        workspace_scope: None,
    }];
    /// A 401 rather than a 403, matching the Zig refusal: the caller has not
    /// PROVEN the thing this route needs proven, and a capability they could be
    /// granted would not change it.
    const CODE: ErrorCode = error_code::AUTH_UNAUTHORIZED;
    const DETAIL: &'static str = DETAIL_NOT_DASHBOARD;
    const EVENT: &'static str = "session_credential_required";
}

/// A browser session only, refusing the way the credential mint refuses.
///
/// Admits exactly what [`DashboardClass`] admits and is a separate policy,
/// because the two refuse differently — 403 with its own sentence, where the
/// dashboard answers 401 with another — and both spellings are pinned to the
/// Zig handlers a client already branches on.
///
/// # Why minting costs a browser session every time
///
/// A credential that can mint another is self-renewing: each mints the next
/// under a machine name of the caller's choosing, revoking any single row
/// leaves its siblings live, and the account holder cannot tell how many exist.
/// That turns one compromised login — a session token good for about a minute —
/// into permanent access outliving every remedy short of deleting the user. A
/// browser sign-in is the one step a stolen credential cannot replay.
#[derive(Debug, Clone, Copy)]
pub struct FreshSessionClass;

impl ClassPolicy for FreshSessionClass {
    const ADMITS: &'static [PersonCredential] = &[PersonCredential::SessionToken {
        workspace_scope: None,
    }];
    const CODE: ErrorCode = error_code::AUTH_FORBIDDEN;
    const DETAIL: &'static str = DETAIL_SESSION_REQUIRED;
    const EVENT: &'static str = "cli_credential_session_required";
}

/// A human acting for themselves — a browser session, or their own terminal.
///
/// Broader than [`FreshSessionClass`] by one class and narrower than
/// [`AnyClass`] by one. Revoking stays open to an `afc_` credential because a
/// terminal must be able to end its own access without a browser; it stays
/// closed to a tenant api-key because an organisation's credential must not
/// manage a person's.
#[derive(Debug, Clone, Copy)]
pub struct HumanClass;

impl ClassPolicy for HumanClass {
    const ADMITS: &'static [PersonCredential] = &[
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        PersonCredential::CliCredential,
    ];
    const CODE: ErrorCode = error_code::AUTH_FORBIDDEN;
    const DETAIL: &'static str = DETAIL_PERSON_REQUIRED;
    const EVENT: &'static str = "cli_credential_person_required";
}

/// A person whose credential class satisfied `P`.
///
/// The `PhantomData` carries the policy at the type level and nothing at
/// runtime, so `Proven<DashboardClass>` and `Proven<HumanClass>` are different
/// types to the compiler and the same bytes to the machine.
///
/// Both fields are PRIVATE, where the four extractors this replaced each had a
/// public one. That was the weaker choice: a public field lets any code build
/// `DashboardIdentity(some_api_key_person)` and hold a value whose entire
/// meaning is "this class was checked". Now [`FromRequestParts`] is the only
/// constructor, so holding one IS the proof — the same argument the value
/// newtypes in `afd_tenant` make for keeping their contents private.
#[derive(Debug, Clone)]
pub struct Proven<P: ClassPolicy>(Person, PhantomData<P>);

impl<P: ClassPolicy> Proven<P> {
    /// The person behind this request.
    #[must_use]
    pub const fn person(&self) -> &Person {
        &self.0
    }

    /// The identity-provider subject, which is what a session is owned BY.
    #[must_use]
    pub fn subject(&self) -> &str {
        self.0.subject().as_str()
    }
}

impl<S: Send + Sync, P: ClassPolicy> FromRequestParts<S> for Proven<P> {
    type Rejection = Response;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        std::future::ready(match person(parts) {
            Some(person) if admits::<P>(person.credential()) => Ok(Self(person, PhantomData)),
            Some(_refused) => Err(refuse(P::CODE, P::DETAIL, P::EVENT)),
            None => Err(unproven()),
        })
    }
}

/// Whether `P` admits `credential`'s class.
///
/// Compares the DISCRIMINANT rather than the value, because a session token
/// carries a workspace ceiling and two sessions with different ceilings are the
/// same class. A `==` here would refuse every session narrowed to a workspace.
fn admits<P: ClassPolicy>(credential: &PersonCredential) -> bool {
    P::ADMITS
        .iter()
        .any(|admitted| std::mem::discriminant(admitted) == std::mem::discriminant(credential))
}

/// A person, whatever they proved it with.
pub type PersonIdentity = Proven<AnyClass>;

/// A person acting through a browser session, on the device-flow surface.
pub type DashboardIdentity = Proven<DashboardClass>;

/// A person proving a browser sign-in, for the verb that mints a credential.
pub type FreshSession = Proven<FreshSessionClass>;

/// A human acting for themselves, for the verb that revokes one.
pub type HumanIdentity = Proven<HumanClass>;

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
