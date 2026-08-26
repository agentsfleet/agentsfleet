//! The request builders every runner-plane suite sends.
//!
//! Separate from `fleet_fixtures.rs` because they are a different kind of
//! thing: that file owns a DATABASE — creating one per test, applying the
//! schema, dropping it — and these own no resource at all. They are pure
//! constructors, which is why they need none of its imports and why a suite
//! that drives the store without touching Postgres can still use them.
#![allow(
    dead_code,
    reason = "test support: shared by several test binaries, each using a subset"
)]

use std::borrow::Cow;

use afd_wire::runner::{
    AssignedPolicy, CapabilityReport, NetworkPolicy, RegisterRequest, SandboxTier,
};

/// A fixed instant, so a row's stamps are asserted rather than observed.
pub(crate) const ENROLLED_AT: i64 = 1_760_000_000_000;

/// How far this suite advances between beats.
///
/// Any value inside the lapse threshold would do — what these tests turn on is
/// whether a beat is INSIDE the freshness window or past it, never the exact
/// distance. Named rather than spelled at each call site so that intent reads
/// off the name (RULE UFS), and so the one test that deliberately steps OUTSIDE
/// the window is visibly different from the ones that do not.
pub(crate) const ONE_BEAT_MS: i64 = 1_000;

/// An enrolment at `tier` under `network`, asking for `workers`.
pub(crate) fn enrolment(
    tier: SandboxTier,
    network: NetworkPolicy,
    workers: u32,
) -> RegisterRequest<'static> {
    RegisterRequest {
        host_id: Cow::Borrowed("host-01.fixture.test"),
        assigned_policy: AssignedPolicy {
            sandbox_tier: tier,
            network_policy: network,
            registry_allowlist: vec![Cow::Borrowed("registry.npmjs.org")],
            worker_count: workers,
            extra_binds: Vec::new(),
        },
        labels: vec![Cow::Borrowed("fixture")],
    }
}

/// A host that proves everything.
pub(crate) fn capable() -> CapabilityReport<'static> {
    CapabilityReport {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec![
            Cow::Borrowed("cpu"),
            Cow::Borrowed("memory"),
            Cow::Borrowed("pids"),
        ],
        bubblewrap: true,
        egress_enforcement: true,
    }
}

/// The body a runner sends to mint a credential.
///
/// A builder rather than a literal at each call site, because the shape is
/// three fields of which two are strings — exactly where a caller binds the
/// lease id and the integration name the wrong way round and gets a refusal
/// that looks like a real one.
pub(crate) fn mint<'a>(
    lease_id: &'a str,
    integration: &'a str,
) -> afd_wire::credentials::MintCredentialRequest<'a> {
    afd_wire::credentials::MintCredentialRequest {
        lease_id: lease_id.into(),
        integration: integration.into(),
        // No narrowing: the fleet's binding is what scopes a mint, and a scope
        // on the wire would be a second opinion about reach.
        scope: None,
    }
}
