//! The egress an assignment names, for the suites that bind a lease's egress
//! without the network; they resolve through `afr_egress`'s `FakeResolver`.

use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};

use crate::egress::Egress;

/// The egress an assignment of `policy` with `registry` as its baseline names.
pub(crate) fn assigned(policy: NetworkPolicy, registry: &[&'static str]) -> Egress {
    Egress::assigned(&AssignedPolicy {
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: policy,
        registry_allowlist: registry.iter().copied().map(Into::into).collect(),
        worker_count: 1,
        extra_binds: Vec::new(),
    })
}
