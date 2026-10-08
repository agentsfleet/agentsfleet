//! A resolver answering from a table, and the egress an assignment names, for
//! the suites that bind a lease's egress without the network.

use std::collections::HashMap;
use std::io;
use std::net::IpAddr;
use std::sync::Arc;

use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};

use crate::egress::{Egress, Resolve};

/// What a host outside the table answers with.
const NO_SUCH_HOST: &str = "no such host";

/// A resolver whose every answer is in its table; a host outside it does not
/// resolve.
#[derive(Debug, Clone, Default)]
pub(crate) struct FakeResolver(Arc<HashMap<String, Vec<IpAddr>>>);

impl FakeResolver {
    /// A resolver answering each host in `table` with its addresses.
    pub(crate) fn answering(table: &[(&str, &[IpAddr])]) -> Self {
        Self(Arc::new(
            table
                .iter()
                .map(|(host, addresses)| ((*host).to_owned(), addresses.to_vec()))
                .collect(),
        ))
    }
}

#[async_trait::async_trait]
impl Resolve for FakeResolver {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        self.0
            .get(host)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NO_SUCH_HOST))
    }
}

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
