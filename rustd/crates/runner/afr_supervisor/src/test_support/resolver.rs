//! The egress an assignment names, for the suites that bind a lease's egress
//! without the network; they resolve through `af_egress`'s `FakeResolver`.

use std::io;
use std::net::IpAddr;
use std::time::Duration;

use af_egress::Resolve;
use af_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};

use super::FakeResolver;
use crate::egress::Egress;

/// A [`FakeResolver`] that answers each host only after `takes`, so a suite
/// sees the egress bind's time on the lease's line.
#[derive(Debug, Clone)]
pub(crate) struct Stalled {
    pub(crate) answers: FakeResolver,
    pub(crate) takes: Duration,
}

#[async_trait::async_trait]
impl Resolve for Stalled {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        tokio::time::sleep(self.takes).await;
        self.answers.resolve(host).await
    }
}

/// The egress an assignment of `policy` with `registry` as its baseline names.
pub(crate) fn assigned(policy: NetworkPolicy, registry: &[&'static str]) -> Egress {
    Egress::assigned(&AssignedPolicy {
        durable_tasks_enabled: af_wire::runner::DURABLE_TASKS_DEFAULT_ENABLED,
        decision_model_id: None,
        sandbox_tier: SandboxTier::LandlockFull,
        network_policy: policy,
        registry_allowlist: registry.iter().copied().map(Into::into).collect(),
        worker_count: 1,
        extra_binds: Vec::new(),
    })
}
