//! Dimension 4.4 — what the capability gate allows, refuses, and says.
//!
//! The gate is the whole of authorization's capability axis, so its failure
//! modes are the ones worth spending tests on: a refusal that does not fail
//! closed is a capability everyone holds, and a refusal a caller cannot act on
//! is a support ticket.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_auth::principal::{Person, PersonCredential, Principal, Runner, Subject};
use afd_auth::scope::{Scope, ScopeSet, parse_claim};
use afd_auth::{Denied, require_scope};
use afd_core::id::Uuid7;

/// A syntactically valid identifier — the gate never reads it.
const ID: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
const OTHER_ID: &str = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a6c";

fn uuid(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a valid v7 identifier")
}

fn person(claim: &str) -> Principal {
    Principal::Person(Person::new(
        PersonCredential::CliCredential,
        uuid(ID),
        Subject::new("user_2abcdef").expect("a non-blank subject"),
        parse_claim(claim),
    ))
}

/// Holding a required scope allows the request; holding none refuses it.
#[test]
fn test_the_gate_is_any_of_and_refuses_when_none_are_held() {
    const REQUIRED: &[Scope] = &[Scope::FleetRead];

    require_scope(&person("fleet:read"), REQUIRED).expect("the exact scope allows");
    require_scope(&person("fleet:admin"), REQUIRED).expect("a higher rung allows");

    let denied = require_scope(&person("billing:read"), REQUIRED).expect_err("unrelated refuses");
    assert_eq!(denied.required(), REQUIRED);
}

/// Any-of means one is enough, not all of them.
#[test]
fn test_one_of_several_requirements_is_enough() {
    const REQUIRED: &[Scope] = &[Scope::FleetAdmin, Scope::BillingRead];

    require_scope(&person("billing:read"), REQUIRED).expect("the second requirement allows");
    require_scope(&person("fleet:admin"), REQUIRED).expect("the first requirement allows");
    require_scope(&person("fleet:admin billing:read"), REQUIRED).expect("holding both allows");
}

/// An empty requirement is authenticated-only, not forbidden.
#[test]
fn test_a_route_naming_no_capability_allows_any_principal() {
    const NONE: &[Scope] = &[];

    require_scope(&person(""), NONE).expect("an empty requirement never refuses");
    require_scope(&Principal::Runner(Runner::new(uuid(ID), false)), NONE)
        .expect("including for a machine");
}

/// Every way a claim can be useless ends in a refusal, never a pass.
///
/// This is the direction that matters. A claim the provider never wrote, one
/// that is malformed, and one resolved for a subject the provider no longer
/// knows all arrive as the empty set, and all three must refuse.
#[test]
fn test_an_unusable_claim_fails_closed() {
    const REQUIRED: &[Scope] = &[Scope::FleetRead];

    for claim in [
        "",
        "   ",
        "fleet:destroy",
        "FLEET:READ",
        "fleet:read\tsecret:read",
    ] {
        let denied = require_scope(&person(claim), REQUIRED)
            .expect_err("a claim granting nothing must refuse");
        assert_eq!(denied.code().as_str(), "UZ-AUTH-022");
    }
}

/// The refusal names the whole requirement, because any one of them satisfies.
///
/// Naming a single scope the caller lacks would tell them to obtain that one
/// when another would also have worked. The rendering matches the Zig daemon's
/// `"Requires scope a or b"` — this is client-visible text on a live surface,
/// not an internal message.
#[test]
fn test_a_refusal_names_every_scope_that_would_satisfy_it() {
    const ONE: &[Scope] = &[Scope::FleetRead];
    const TWO: &[Scope] = &[Scope::FleetRead, Scope::FleetWrite];
    const THREE: &[Scope] = &[Scope::FleetRead, Scope::FleetWrite, Scope::BillingRead];

    let rendered = |required: &'static [Scope]| {
        require_scope(&person("stream:read"), required)
            .expect_err("stream:read satisfies none of these")
            .to_string()
    };

    assert_eq!(rendered(ONE), "Requires scope fleet:read");
    assert_eq!(rendered(TWO), "Requires scope fleet:read or fleet:write");
    assert_eq!(
        rendered(THREE),
        "Requires scope fleet:read or fleet:write or billing:read"
    );
}

/// A refusal is a real error, so it composes with the rest of the error surface.
#[test]
fn test_a_refusal_is_an_error() {
    const REQUIRED: &[Scope] = &[Scope::SecretWrite];
    let denied: Denied = require_scope(&person("fleet:read"), REQUIRED).expect_err("refused");

    let as_error: &dyn std::error::Error = &denied;
    assert!(as_error.to_string().contains("secret:write"));
    assert!(as_error.source().is_none(), "a refusal wraps nothing");
}

/// A runner holds its own plane and nothing else, and cannot be built holding
/// anything else.
///
/// The Zig daemon assigns `RUNNER_SCOPES` at its one construction site. Here
/// the capabilities are derived from the variant, so there is no assignment to
/// forget — which is what this asserts by constructing a runner and finding it
/// holds exactly `runner:self` without having been told to.
#[test]
fn test_a_runner_holds_only_its_own_plane_by_construction() {
    const TENANT_ROUTE: &[Scope] = &[Scope::FleetRead];
    const RUNNER_ROUTE: &[Scope] = &[Scope::RunnerSelf];

    let runner = Principal::Runner(Runner::new(uuid(ID), false));

    require_scope(&runner, RUNNER_ROUTE).expect("a runner reaches its own plane");
    require_scope(&runner, TENANT_ROUTE).expect_err("a runner must not reach a tenant route");
    assert_eq!(
        runner.scopes(),
        ScopeSet::from_scopes(&[Scope::RunnerSelf]),
        "a runner's capabilities are derived from its variant, not assigned"
    );
}

/// A person cannot reach the runner plane, however capable they are.
#[test]
fn test_the_most_capable_person_cannot_reach_the_runner_plane() {
    const RUNNER_ROUTE: &[Scope] = &[Scope::RunnerSelf];

    let every_scope = Scope::ALL
        .iter()
        .filter(|s| **s != Scope::RunnerSelf)
        .map(|s| s.wire())
        .collect::<Vec<_>>()
        .join(" ");

    require_scope(&person(&every_scope), RUNNER_ROUTE)
        .expect_err("holding every other capability must not open the machine plane");
}

/// A runner has no tenant, and a person always has one.
#[test]
fn test_a_runner_carries_no_tenant_authority() {
    let runner = Principal::Runner(Runner::new(uuid(ID), true));
    assert!(runner.tenant().is_none());
    assert!(runner.person().is_none());
    assert!(runner.runner().expect("is a runner").is_degraded());

    let caller = person("fleet:read");
    assert_eq!(
        caller.tenant().expect("a person acts in a tenant").as_str(),
        ID
    );
    assert!(caller.runner().is_none());
}

/// Only a session token can be confined to one workspace.
#[test]
fn test_a_workspace_ceiling_exists_only_where_a_claim_can_carry_one() {
    let confined = Person::new(
        PersonCredential::SessionToken {
            workspace_scope: Some(uuid(OTHER_ID)),
        },
        uuid(ID),
        Subject::new("user_2abcdef").expect("subject"),
        parse_claim("fleet:admin"),
    );
    assert_eq!(
        confined.workspace_scope().expect("confined").as_str(),
        OTHER_ID
    );

    let unconfined = Person::new(
        PersonCredential::SessionToken {
            workspace_scope: None,
        },
        uuid(ID),
        Subject::new("user_2abcdef").expect("subject"),
        parse_claim("fleet:admin"),
    );
    assert!(unconfined.workspace_scope().is_none());

    for credential in [
        PersonCredential::TenantApiKey,
        PersonCredential::CliCredential,
    ] {
        let other = Person::new(
            credential,
            uuid(ID),
            Subject::new("user_2abcdef").expect("subject"),
            parse_claim("fleet:admin"),
        );
        assert!(
            other.workspace_scope().is_none(),
            "only a session token carries a workspace claim"
        );
    }
}

/// A blank subject is refused where it is built, not where it fails to resolve.
#[test]
fn test_a_blank_subject_is_refused_at_construction() {
    for blank in ["", " ", "\t", "\n  "] {
        assert!(
            Subject::new(blank).is_err(),
            "{blank:?} was accepted as an identity"
        );
    }
    let subject = Subject::new("user_2abcdef").expect("a real subject");
    assert_eq!(subject.as_str(), "user_2abcdef");
    assert_eq!(subject.to_string(), "user_2abcdef");
}

/// A principal reports back exactly what it was built from.
///
/// The accessors are the whole read surface the middleware and the ownership
/// check see, so an accessor that reports the wrong field is a principal acting
/// as somebody else. Asserted here rather than left for §5 to discover: an
/// untested accessor is dead code that looks like coverage.
#[test]
fn test_a_principal_reports_the_identity_it_was_built_from() {
    let subject = Subject::new("user_2abcdef").expect("subject");
    let built = Person::new(
        PersonCredential::TenantApiKey,
        uuid(ID),
        subject.clone(),
        parse_claim("fleet:read"),
    );

    assert_eq!(built.credential(), &PersonCredential::TenantApiKey);
    assert_eq!(built.subject(), &subject);
    assert_eq!(built.subject().as_str(), "user_2abcdef");
    assert_eq!(built.tenant().as_str(), ID);

    let principal = Principal::Person(built);
    let seen = principal.person().expect("a person principal");
    assert_eq!(seen.credential(), &PersonCredential::TenantApiKey);
    assert_eq!(
        seen.subject().as_str(),
        "user_2abcdef",
        "the subject the provider resolved capabilities for must survive"
    );

    let runner = Runner::new(uuid(OTHER_ID), false);
    assert_eq!(runner.id().as_str(), OTHER_ID);
    assert!(!runner.is_degraded());
    assert_eq!(
        Principal::Runner(runner)
            .runner()
            .expect("a runner")
            .id()
            .as_str(),
        OTHER_ID
    );
}

/// The credential class survives authentication, because one rule needs it.
///
/// A user-scoped route accepts a terminal credential and refuses a tenant-wide
/// api-key even when both resolve to the same person holding the same
/// capabilities — so the class cannot be discarded once the scopes are known.
#[test]
fn test_the_credential_class_is_distinguishable_after_authentication() {
    let of = |credential: PersonCredential| {
        Person::new(
            credential,
            uuid(ID),
            Subject::new("user_2abcdef").expect("subject"),
            parse_claim("fleet:admin"),
        )
    };

    let terminal = of(PersonCredential::CliCredential);
    let key = of(PersonCredential::TenantApiKey);

    assert_eq!(
        Principal::Person(terminal.clone()).scopes(),
        Principal::Person(key.clone()).scopes(),
        "the same person resolves the same capabilities either way"
    );
    assert_ne!(
        terminal.credential(),
        key.credential(),
        "and the classes stay distinguishable regardless"
    );
}
