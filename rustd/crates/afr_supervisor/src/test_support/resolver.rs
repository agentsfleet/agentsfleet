//! A resolver answering from a table, and the egress an assignment names, for
//! the suites that bind a lease's egress without the network.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};

use afd_wire::runner::{AssignedPolicy, NetworkPolicy, SandboxTier};

use crate::egress::{Egress, Resolve};

/// What a host outside the table answers with.
const NO_SUCH_HOST: &str = "no such host";

/// A resolver whose every answer is in its table; a host outside it does not
/// resolve. A host with several answers gives them in turn, then keeps
/// giving the last, as a name a DNS change moved.
#[derive(Debug, Clone, Default)]
pub(crate) struct FakeResolver(Arc<Mutex<HashMap<String, VecDeque<Vec<IpAddr>>>>>);

impl FakeResolver {
    /// A resolver answering each host in `table` with its addresses.
    pub(crate) fn answering(table: &[(&str, &[IpAddr])]) -> Self {
        Self::moving(
            &table
                .iter()
                .map(|&(host, addresses)| (host, vec![addresses]))
                .collect::<Vec<_>>(),
        )
    }

    /// A resolver answering each host in `table` with each of its answers in
    /// turn.
    pub(crate) fn moving(table: &[(&str, Vec<&[IpAddr]>)]) -> Self {
        let answers = table
            .iter()
            .map(|(host, answers)| {
                let answers = answers.iter().map(|answer| answer.to_vec()).collect();
                ((*host).to_owned(), answers)
            })
            .collect();
        Self(Arc::new(Mutex::new(answers)))
    }
}

#[async_trait::async_trait]
impl Resolve for FakeResolver {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        let mut table = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let answers = table
            .get_mut(host)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, NO_SUCH_HOST))?;
        let answer = if answers.len() > 1 {
            answers.pop_front()
        } else {
            answers.front().cloned()
        };
        Ok(answer.unwrap_or_default())
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
