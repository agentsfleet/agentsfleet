//! The daemon's storage client: direct, bounded, with no redirect or retry,
//! and a resolver that checks a connection's whole DNS answer against the
//! shared address policy, so a stored URL cannot reach a private address.
//!
//! Checkpoint storage and GitHub share it. It was once built from setup's
//! model transport; the daemon makes no model call, so it has its own.

use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// The longest one storage request may take, end to end.
const TIMEOUT: Duration = Duration::from_secs(300);
/// The longest a storage connection may take to open.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest a storage response may go silent.
const READ_TIMEOUT: Duration = Duration::from_secs(300);

/// Builds the storage client.
///
/// # Errors
/// Reports a client or TLS initialisation failure.
pub(super) fn storage_client() -> reqwest::Result<reqwest::Client> {
    afd_identity::direct(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .dns_resolver(Arc::new(Guarded))
        .build()
}

/// Refuses a name whose answer is empty or holds any blocked address.
#[derive(Debug)]
struct Guarded;

impl Resolve for Guarded {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addresses.is_empty()
                || addresses
                    .iter()
                    .any(|address| af_core::net::is_blocked(address.ip()))
            {
                return Err(std::io::Error::other("storage destination refused").into());
            }
            let resolved: Addrs = Box::new(addresses.into_iter());
            Ok(resolved)
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures must fail loudly on invalid setup"
)]
mod tests {
    use super::*;
    use std::{io::ErrorKind, net::SocketAddr};

    #[tokio::test]
    async fn public_numeric_destination_returns_the_resolved_address() {
        let addresses: Vec<_> = Guarded
            .resolve("1.1.1.1".parse().unwrap())
            .await
            .unwrap()
            .collect();
        assert_eq!(addresses, ["1.1.1.1:0".parse::<SocketAddr>().unwrap()]);
    }

    #[tokio::test]
    async fn private_loopback_metadata_and_unspecified_destinations_are_refused() {
        for host in [
            "localhost",
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
        ] {
            let result = Guarded.resolve(host.parse().unwrap()).await;
            let failure = result.err().expect("private destination must be refused");
            let failure = failure.downcast_ref::<std::io::Error>().unwrap();
            assert_eq!(failure.kind(), ErrorKind::Other);
            assert_eq!(failure.to_string(), "storage destination refused");
        }
    }

    #[test]
    fn the_storage_client_builds_without_a_request() {
        let _client = storage_client().expect("the storage client builds without a request");
    }
}
