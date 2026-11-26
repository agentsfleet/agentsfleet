#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;

use super::{Limits, ResourceFacts, SandboxSpec};
use crate::network::{Allowlist, Network};

/// A spec names no network until it is told one, and then names that one,
/// keeping its identifier and grant.
#[test]
fn test_a_spec_is_isolated_until_told_otherwise() {
    let id = Uuid7::parse("0198f0c2-7a3e-7c1d-9b2e-4f6a8c0d1e2f").expect("a v7 identifier");
    let granted = ResourceFacts::ENFORCED
        .grant(SandboxEngine::Bubblewrap, Limits::DEFAULT)
        .expect("every cap enforced");
    let allowlist = Allowlist::new(Vec::new()).expect("an empty allowlist is an allowlist");
    let isolated = SandboxSpec::new(&id, granted);

    let shared = isolated.with_network(Network::Host);
    let allowed = isolated.with_network(Network::Allowed(&allowlist));

    assert_eq!(isolated.network, Network::Isolated);
    assert_eq!(shared.network, Network::Host);
    assert_eq!(allowed.network, Network::Allowed(&allowlist));
    assert_eq!(
        (allowed.id, allowed.granted),
        (isolated.id, isolated.granted)
    );
}
