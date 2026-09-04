//! Dimensions 4.3 and 4.4 — the ladder, and what a gate does with it.
//!
//! Every assertion the retired daemon's `auth/scopes_test.zig` made, run against
//! the Rust catalogue with the same inputs. The Zig suite is the oracle for the
//! same reason it is in `afd_crypto` and `afd_db`: it encodes what the deployed
//! daemon already enforces, so re-running its claims proves more than a fixture
//! written here could. The catalogue is a wire contract shared verbatim with
//! the identity provider, and a divergence is a capability granted by one
//! binary and refused by the other.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use afd_auth::scope::{
    RUNNER_SCOPES, Scope, ScopeSet, TENANT_OWNER_GRANT, parse_claim, signup_owner_claim,
};

/// Every scope round-trips through its claim value, and no two share one.
///
/// The wire string is matched verbatim in the provider's token template, so a
/// duplicate would make two capabilities indistinguishable on the wire and a
/// failed round-trip would make a granted scope unparseable.
#[test]
fn test_the_catalogue_round_trips_and_its_claim_values_are_distinct() {
    let mut seen = BTreeSet::new();
    for scope in Scope::ALL {
        let wire = scope.wire();
        assert!(
            seen.insert(wire),
            "{wire} is the claim value of two different scopes"
        );
        assert_eq!(
            Scope::from_wire(wire),
            Some(scope),
            "{wire} did not round-trip"
        );
        assert!(
            wire.contains(':'),
            "{wire} is not a resource:action spelling"
        );
    }
    assert_eq!(seen.len(), Scope::ALL.len());
}

/// A claim value the catalogue does not know names no scope.
#[test]
fn test_an_unknown_claim_value_names_nothing() {
    for unknown in ["fleet:destroy", "wat", "", "fleet", "FLEET:READ"] {
        assert_eq!(Scope::from_wire(unknown), None, "{unknown:?} was accepted");
    }
}

/// `admin` subsumes `write` subsumes `read`, and only where the ladder says so.
#[test]
fn test_the_hierarchy_subsumes_lower_rungs() {
    let held = parse_claim("fleet:admin");
    assert!(held.contains(Scope::FleetAdmin));
    assert!(held.contains(Scope::FleetWrite));
    assert!(held.contains(Scope::FleetRead));

    let write = parse_claim("apikey:write");
    assert!(write.contains(Scope::ApikeyWrite));
    assert!(write.contains(Scope::ApikeyRead));
    assert!(
        !write.contains(Scope::ApikeyAdmin),
        "a ladder climbs down, never up"
    );

    // Deciding an approval implies seeing the inbox.
    let resolve = parse_claim("approval:resolve");
    assert!(resolve.contains(Scope::ApprovalRead));
}

/// The two library scopes look laddered and are not.
///
/// They share a spelling convention — `library:write` and
/// `platform-library:write` — which is exactly what a string-prefix hierarchy
/// would ladder together, handing a workspace owner the platform catalogue.
/// The hierarchy is data for this reason.
#[test]
fn test_the_library_scopes_are_independent_despite_their_spelling() {
    let platform = parse_claim("platform-library:write");
    assert!(platform.contains(Scope::PlatformLibraryWrite));
    assert!(!platform.contains(Scope::LibraryWrite));

    let tenant = parse_claim("library:write");
    assert!(tenant.contains(Scope::LibraryWrite));
    assert!(!tenant.contains(Scope::PlatformLibraryWrite));
}

/// A discrete verb grants itself and nothing else.
#[test]
fn test_a_discrete_verb_expands_to_itself() {
    let enroll = parse_claim("runner:enroll");
    // Set equality rather than a count beside a membership check: the property
    // is "this and nothing else", and stating it as one assertion is what makes
    // an accidental widening fail here rather than pass a count that moved.
    assert_eq!(enroll, ScopeSet::from_scopes(&[Scope::RunnerEnroll]));
    assert!(
        !enroll.contains(Scope::RunnerRead),
        "enrolling a runner is not reading one — they are separately revocable"
    );
}

/// A claim is parsed into the closure of what it granted.
#[test]
fn test_a_claim_parses_into_its_downward_closure() {
    let held = parse_claim("fleet:read secret:write");
    assert!(held.contains(Scope::FleetRead));
    assert!(held.contains(Scope::SecretWrite));
    assert!(held.contains(Scope::SecretRead), "write subsumes read");
    assert!(
        !held.contains(Scope::FleetWrite),
        "read does not imply write"
    );
}

/// An empty or wholly unknown claim grants nothing, and a mixed one grants only
/// what it named.
#[test]
fn test_an_unparseable_claim_grants_nothing_rather_than_failing() {
    assert!(parse_claim("").is_empty());
    assert!(parse_claim("   ").is_empty());
    assert!(parse_claim("fleet:destroy wat").is_empty());

    let mixed = parse_claim("fleet:destroy fleet:read wat");
    assert_eq!(
        mixed,
        ScopeSet::from_scopes(&[Scope::FleetRead]),
        "the typos grant nothing, the real one grants"
    );
}

/// The delimiter is one ASCII space, matching `tokenizeScalar(u8, raw, ' ')`.
///
/// Splitting on all whitespace would be the friendlier parse and the wrong one:
/// the Rust daemon would then grant a capability from a tab-separated claim
/// that the Zig daemon reads as one unknown token and refuses. Two binaries
/// disagreeing about a capability is worse than both refusing a malformed
/// claim, so this pins the stricter, shared behaviour.
#[test]
fn test_only_a_space_delimits_a_claim() {
    let both = ScopeSet::from_scopes(&[Scope::FleetRead, Scope::SecretRead]);
    assert_eq!(parse_claim("fleet:read secret:read"), both);

    for separator in ["\t", "\n", "\r\n", ","] {
        let claim = format!("fleet:read{separator}secret:read");
        assert!(
            parse_claim(&claim).is_empty(),
            "{separator:?} must not delimit — it made {claim:?} grant something"
        );
    }

    // Repeated spaces are not empty tokens.
    assert_eq!(parse_claim("fleet:read   secret:read"), both);
    assert_eq!(
        parse_claim(" fleet:read "),
        ScopeSet::from_scopes(&[Scope::FleetRead])
    );
}

/// Any-of with the hierarchy already expanded, and the fail-closed direction.
#[test]
fn test_a_gate_is_any_of_over_the_expanded_set() {
    let route_any_of = [Scope::FleetRead];

    assert!(parse_claim("fleet:admin").satisfies_any(&route_any_of));
    assert!(parse_claim("fleet:read").satisfies_any(&route_any_of));
    assert!(
        !parse_claim("").satisfies_any(&route_any_of),
        "an empty set must fail closed against a real requirement"
    );
    assert!(
        !parse_claim("fleet:write").satisfies_any(&[Scope::FleetAdmin]),
        "holding write must not satisfy an admin gate"
    );
    assert!(
        parse_claim("").satisfies_any(&[]),
        "a route naming no capability is authenticated-only, not forbidden"
    );

    // Any-of means one is enough, not all.
    assert!(parse_claim("billing:read").satisfies_any(&[Scope::FleetAdmin, Scope::BillingRead]));
}

/// The runner plane carries exactly one capability.
///
/// A runner receives every tenant's inline secrets, so the blast radius of it
/// reaching a tenant route is the whole product. It holds `runner:self` and
/// nothing else, and nothing else holds `runner:self`.
#[test]
fn test_the_runner_credential_carries_only_its_own_plane() {
    assert_eq!(RUNNER_SCOPES, ScopeSet::from_scopes(&[Scope::RunnerSelf]));

    for scope in Scope::ALL {
        if scope == Scope::RunnerSelf {
            continue;
        }
        assert!(
            !RUNNER_SCOPES.contains(scope),
            "a runner must not hold {}",
            scope.wire()
        );
    }

    let owner = ScopeSet::from_scopes(&TENANT_OWNER_GRANT);
    assert!(
        !owner.contains(Scope::RunnerSelf),
        "a person must not hold the machine plane"
    );
}

/// The signup grant provisions a tenant owner and stops short of the platform.
#[test]
fn test_the_signup_grant_is_a_tenant_owner_and_not_an_operator() {
    let owner = ScopeSet::from_scopes(&TENANT_OWNER_GRANT);

    assert!(owner.contains(Scope::FleetAdmin));
    assert!(
        owner.contains(Scope::FleetRead),
        "closure applies to a grant"
    );
    assert!(owner.contains(Scope::ScheduleWrite));
    assert!(owner.contains(Scope::ScheduleRead));
    assert!(owner.contains(Scope::SecretWrite));
    assert!(owner.contains(Scope::WorkspaceAdmin));
    assert!(owner.contains(Scope::LibraryWrite));

    for withheld in [
        Scope::PlatformLibraryWrite,
        Scope::RunnerEnroll,
        Scope::WorkspaceAny,
        Scope::ModelAdmin,
        Scope::PlatformKeyAdmin,
        Scope::StreamRead,
    ] {
        assert!(
            !owner.contains(withheld),
            "an admin must not be handed {} at signup",
            withheld.wire()
        );
    }
}

/// The seeded claim is what the parser reads back, with lower rungs implied.
#[test]
fn test_the_signup_claim_round_trips_through_the_parser() {
    let claim = signup_owner_claim();
    let parsed = parse_claim(&claim);
    let built = ScopeSet::from_scopes(&TENANT_OWNER_GRANT);

    assert_eq!(parsed, built, "the seeded claim must mean what it grants");
    for scope in TENANT_OWNER_GRANT {
        assert!(
            claim.contains(scope.wire()),
            "{} is granted but not written",
            scope.wire()
        );
    }
    assert!(
        !claim.contains("fleet:read"),
        "lower rungs are implied by the parser, not spelled into the claim"
    );
}

/// Iteration reports exactly what membership reports.
#[test]
fn test_iteration_agrees_with_membership() {
    let held = parse_claim("fleet:admin billing:read");
    let listed: Vec<Scope> = held.iter().collect();

    // Pins content, cardinality AND catalogue order in one assertion — the
    // expansion of `fleet:admin` down its ladder, plus the discrete verb.
    assert_eq!(
        listed,
        vec![
            Scope::FleetRead,
            Scope::FleetWrite,
            Scope::FleetAdmin,
            Scope::BillingRead,
        ]
    );
    for scope in Scope::ALL {
        assert_eq!(
            listed.contains(&scope),
            held.contains(scope),
            "{} disagreed between iter and contains",
            scope.wire()
        );
    }
    assert!(ScopeSet::EMPTY.iter().next().is_none());
}

/// `docs/AUTH.md`'s Scope catalogue lists every claim value this crate defines.
///
/// Mirrors the Zig test of the same claim, and reads the catalogue rather than
/// a hand-typed list so a scope added here fails the moment the doc goes stale.
#[test]
fn test_every_claim_value_appears_in_the_auth_doc() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("crates/afd_auth is three levels below the repository root");
    let doc = std::fs::read_to_string(root.join("docs/AUTH.md"))
        .expect("docs/AUTH.md must be readable from the repository root");

    for scope in Scope::ALL {
        assert!(
            doc.contains(scope.wire()),
            "{} is missing from docs/AUTH.md",
            scope.wire()
        );
    }
}
