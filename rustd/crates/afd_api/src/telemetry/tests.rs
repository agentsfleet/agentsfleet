//! Which product event a refusal becomes, decided by its registry code.
//!
//! The mapping is the only judgment in this layer — everything else is reading
//! an extension and handing it on — so it is proven here rather than through
//! HTTP, where a router would have to be stood up to assert on a `match`.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_core::error_code;
use afd_observability::Telemetry;

use super::{ACTOR_SYSTEM, telemetry_of};
use crate::envelope::Refused;

/// The person a fixture refusal happened to.
const ACTOR: &str = "user_2telemetry";

/// The workspace they were acting in.
const WORKSPACE: &str = "01924f4e-0000-7000-8000-000000000001";

/// One refusal, as the envelope stamps it onto a response.
fn refused(code: error_code::ErrorCode) -> Refused {
    Refused {
        code,
        detail: "the sentence a client reads".to_owned(),
        request_id: "req_2telemetry".to_owned(),
    }
}

/// A credential this daemon would not take is `auth_rejected`.
///
/// The funnel that asks "how many people could not sign in" reads this event,
/// and it must not have to filter `api_error` by code prefix to find them.
#[test]
fn should_report_a_credential_refusal_as_a_rejected_sign_in() {
    for code in [
        error_code::AUTH_UNAUTHORIZED,
        error_code::AUTH_FORBIDDEN,
        error_code::AUTH_TOKEN_EXPIRED,
        error_code::AUTH_INSUFFICIENT_SCOPE,
    ] {
        let event = telemetry_of(&refused(code), ACTOR.to_owned(), None);
        assert_eq!(event.name(), "auth_rejected", "{}", code.as_str());
    }
}

/// The reason a rejected sign-in carries is the CODE, never the sentence.
///
/// A detail can name a credential shape or a subject, and this record is read
/// by everyone with dashboard access. The code says which wall refused and
/// nothing else.
#[test]
fn should_report_the_code_and_not_the_sentence_as_the_reason() {
    let event = telemetry_of(
        &refused(error_code::AUTH_UNAUTHORIZED),
        ACTOR.to_owned(),
        None,
    );
    let Telemetry::AuthRejected { reason, .. } = event else {
        panic!("a UZ-AUTH code is a rejected sign-in");
    };
    assert_eq!(reason, error_code::AUTH_UNAUTHORIZED.as_str());
    assert!(
        !reason.contains("sentence"),
        "the detail must not travel in the reason"
    );
}

/// A refusal a plan or a policy raised is `entitlement_rejected`.
#[test]
fn should_report_a_boundary_refusal_as_an_entitlement() {
    for code in [
        error_code::REPAIR_SPEND_EXHAUSTED,
        error_code::REPAIR_WRITE_UNAPPROVED,
    ] {
        let event = telemetry_of(&refused(code), ACTOR.to_owned(), Some(WORKSPACE.to_owned()));
        assert_eq!(event.name(), "entitlement_rejected", "{}", code.as_str());
    }
}

/// Everything else is `api_error`, carrying the sentence a client was given.
#[test]
fn should_report_every_other_refusal_as_an_api_error() {
    let event = telemetry_of(
        &refused(error_code::INVALID_REQUEST),
        ACTOR.to_owned(),
        Some(WORKSPACE.to_owned()),
    );
    assert_eq!(event.name(), "api_error");
    let Telemetry::ApiError {
        message,
        workspace_id,
        ..
    } = event
    else {
        panic!("a UZ-REQ code is an api error");
    };
    assert_eq!(message, "the sentence a client reads");
    assert_eq!(workspace_id.as_deref(), Some(WORKSPACE));
}

/// A refusal written before a workspace was resolved omits the key.
///
/// The whole reason `ApiError` carries an `Option`: a `null` would make every
/// pre-workspace refusal a cohort under one dashboard filter.
#[test]
fn should_omit_the_workspace_when_the_refusal_preceded_one() {
    let event = telemetry_of(
        &refused(error_code::INVALID_REQUEST),
        ACTOR_SYSTEM.to_owned(),
        None,
    );
    let Telemetry::ApiError {
        actor,
        workspace_id,
        ..
    } = event
    else {
        panic!("a UZ-REQ code is an api error");
    };
    assert_eq!(workspace_id, None);
    assert_eq!(
        actor, ACTOR_SYSTEM,
        "a refusal with nobody behind it is one non-person, not many"
    );
}

/// A code this mapping does not recognise still reports, as an api error.
///
/// The arm that keeps the layer total. Every declared code has three segments —
/// `afd_core`'s own test proves it — so this is unreachable for a real code,
/// and it must not be a panic on the refusal path if it ever is not.
#[test]
fn should_report_an_unrecognised_family_rather_than_dropping_it() {
    let event = telemetry_of(
        &refused(error_code::INTERNAL_DB_UNAVAILABLE),
        ACTOR.to_owned(),
        None,
    );
    assert_eq!(event.name(), "api_error");
}
