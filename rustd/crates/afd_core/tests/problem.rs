//! Every code a client can receive carries the status and prose it answers with.
//!
//! What this file proves is that the table is TOTAL over the registry — so
//! §5's `application/problem+json` envelope can be assembled from it without a
//! second lookup that could disagree, and no declared code can reach a client
//! as an unknown error.
use std::collections::BTreeSet;

use afd_core::error_code::{self, REGISTRY};
use afd_core::problem::{DOCS_BASE, Problem, entries};

/// The documentation base the site actually serves, stated rather than derived.
const PUBLISHED_DOCS_BASE: &str = "https://docs.agentsfleet.net/api-reference/error-codes#";

/// The table covers the registry exactly — no code without an entry, and no
/// entry for a code that is not declared.
///
/// The first half is what keeps [`Problem::UNKNOWN`] unreachable: a declared
/// code with no entry would answer 500 "Unknown error" to a client, which is
/// the failure this test exists to make impossible. The second half stops a
/// stale entry outliving the code it described.
#[test]
fn test_every_declared_code_has_an_entry_and_no_entry_is_orphaned() {
    for code in REGISTRY {
        let problem = Problem::of(*code);
        assert_eq!(
            problem.code(),
            *code,
            "{} has no entry, so it would answer as an unknown error",
            code.as_str()
        );
    }

    let declared: BTreeSet<_> = REGISTRY.iter().map(|code| code.as_str()).collect();
    for entry in entries() {
        assert!(
            declared.contains(entry.code().as_str()),
            "{} has an entry but is not declared in REGISTRY",
            entry.code().as_str()
        );
    }
    assert_eq!(entries().len(), REGISTRY.len());
}

/// The documentation link is derived from the code, so it cannot point at
/// another code's anchor.
#[test]
fn test_the_docs_link_is_derived_from_the_code() {
    for entry in entries() {
        let uri = entry.docs_uri();
        assert!(uri.starts_with(DOCS_BASE), "{uri}");
        assert!(uri.ends_with(entry.code().as_str()), "{uri}");
    }
    assert_eq!(
        Problem::of(error_code::AUTH_INSUFFICIENT_SCOPE).docs_uri(),
        format!("{DOCS_BASE}UZ-AUTH-022")
    );
    // And the base is the one the documentation site actually serves.
    assert_eq!(
        DOCS_BASE, PUBLISHED_DOCS_BASE,
        "the docs base does not match the one the documentation site serves"
    );
}

/// The statuses the auth plane depends on, stated rather than inferred.
///
/// These four are load-bearing beyond the envelope: `docs/AUTH.md` rests on
/// 022 being a 403 (re-authenticating cannot help), and the runner client
/// classifies 004 as transport loss rather than an auth rejection — which is
/// what stops a datastore outage walking a healthy fleet to shutdown.
#[test]
fn test_the_auth_planes_statuses_are_the_documented_ones() {
    for (code, status) in [
        (error_code::AUTH_INSUFFICIENT_SCOPE, 403),
        (error_code::AUTH_UNAUTHORIZED, 401),
        (error_code::AUTH_TOKEN_EXPIRED, 401),
        (error_code::AUTH_UNAVAILABLE, 503),
        (error_code::AUTH_CLI_CREDENTIAL_REVOKED, 401),
        (error_code::APIKEY_REVOKED, 401),
        (error_code::RUN_INVALID_RUNNER_TOKEN, 401),
        (error_code::RUN_ADMIN_STATE_BLOCKED, 401),
    ] {
        assert_eq!(Problem::of(code).status(), status, "{}", code.as_str());
    }
}

/// An unregistered code degrades to an honest 500 rather than failing.
///
/// Unreachable for a code this workspace declares — the totality test above is
/// what makes that true — but a response is being written when this is reached,
/// and there is nothing better to do than answer.
#[test]
fn test_an_unregistered_code_degrades_to_the_unknown_entry() {
    let stranger = afd_core::error_code::ErrorCode::declare("UZ-NOSUCH-001");
    let problem = Problem::of(stranger);

    assert_eq!(problem, Problem::UNKNOWN);
    assert_eq!(problem.status(), 500);
    assert_eq!(problem.title(), "Unknown error");
    assert!(problem.user_message().is_none());
    assert!(!problem.hint().is_empty());
}
