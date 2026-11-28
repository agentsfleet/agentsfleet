//! Distinct loopback listeners and DNS lookups for multi-origin egress tests.

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::Upstream;

/// DNS queries observed at the actual client resolver for each fixture host.
#[derive(Debug)]
pub struct Lookups(HashMap<&'static str, (SocketAddr, AtomicUsize)>);

impl Lookups {
    /// How many times the HTTP client looked up this host.
    #[must_use]
    pub fn count(&self, host: &str) -> usize {
        self.0
            .get(host)
            .map_or(0, |(_, count)| count.load(Ordering::SeqCst))
    }
}

impl Resolve for Lookups {
    fn resolve(&self, name: Name) -> Resolving {
        let address = self.0.get(name.as_str()).map(|(address, count)| {
            count.fetch_add(1, Ordering::SeqCst);
            *address
        });
        Box::pin(async move {
            let address = address.ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
            Ok(Box::new(std::iter::once(address)) as Addrs)
        })
    }
}

impl Upstream {
    /// A client that resolves each named upstream to its own listener.
    ///
    /// # Panics
    /// The fixture HTTP client cannot be built.
    #[expect(clippy::expect_used, reason = "a test client must build")]
    #[must_use]
    pub fn observed_network(upstreams: &[&Self]) -> (crate::Network, Arc<Lookups>) {
        let routes = upstreams
            .iter()
            .flat_map(|upstream| {
                upstream
                    .hosts
                    .iter()
                    .map(|&host| (host, (upstream.address, AtomicUsize::new(0))))
            })
            .collect();
        let lookups = Arc::new(Lookups(routes));
        let roots = upstreams.iter().map(|upstream| upstream.root.clone());
        let network = crate::Network::observed(roots, Arc::clone(&lookups))
            .expect("the observed client builds");
        (network, lookups)
    }

    /// Accepted TCP sockets, including sockets that failed before sending HTTP.
    #[must_use]
    pub fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
}
