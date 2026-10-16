//! Check the actual connection's complete DNS answer with the shared address policy.

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

#[derive(Debug)]
pub(super) struct Guarded;

impl Resolve for Guarded {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addresses.is_empty()
                || addresses
                    .iter()
                    .any(|address| afd_core::net::is_blocked(address.ip()))
            {
                return Err(std::io::Error::other("authoring destination refused").into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}
