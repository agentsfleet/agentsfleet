//! The one client every egress tool sends through.
//!
//! Built once per runner and shared by every lease. Its resolver refuses a
//! name any of whose addresses is private, loopback or reserved
//! (`afd_core::net`), so an allowlisted host that resolves inside the host's
//! network is never reached; the check runs on each new connection's lookup,
//! which also covers a name that changes its answer between calls. It follows
//! no redirect and reads no proxy from the environment (either would send the
//! request somewhere admission never saw), sends HTTPS only, gives up after a
//! fixed time, and reads at most [`RESPONSE_MAX_BYTES`] of a response.
//!
//! This is where the runner departs from `NullClaw` on purpose: `NullClaw` lets
//! an allowlisted host resolve to a private address, and a runner serving many
//! tenants on one host does not.

use std::error::Error as StdError;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use afd_core::net::is_blocked;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{CONTENT_TYPE, HeaderName, LOCATION};
use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder, Response, Url};

use crate::error::Result;
use crate::refusal::Refusal;
use crate::transport::{Inbound, Outbound, Transport};

/// The most of one response a tool reads: the published tools page's 1 MiB.
pub const RESPONSE_MAX_BYTES: usize = 1 << 20;

/// How long one request may take, connection to last byte.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// What the runner calls itself; GitHub refuses a request that names nothing.
const USER_AGENT: &str = concat!("agentsfleet-runner/", env!("CARGO_PKG_VERSION"));

/// Why a request that left brought nothing back, in phrases carrying no URL.
const TIMED_OUT: &str = "the request timed out";
const NOT_CONNECTED: &str = "the connection could not be made";
const NOT_READ: &str = "the response could not be read";

/// The client every egress tool sends through.
#[derive(Debug, Clone)]
pub struct Network {
    client: Client,
}

impl Network {
    /// The production client.
    ///
    /// # Errors
    /// The TLS backend or the client refused the configuration.
    pub fn new() -> Result<Self> {
        Self::build(Client::builder())
    }

    /// A client that reaches each `(host, address)` at that address and
    /// trusts `root` besides the platform's roots, for the integration lane's
    /// fakes. The overrides skip the resolver's guard, which is why this is a
    /// test seam and never a production one.
    ///
    /// # Errors
    /// As [`Network::new`].
    #[cfg(feature = "test-util")]
    pub fn routed(routes: &[(&str, SocketAddr)], root: reqwest::Certificate) -> Result<Self> {
        let builder = routes
            .iter()
            .fold(Client::builder(), |builder, (host, address)| {
                builder.resolve(host, *address)
            });
        Self::build(builder.add_root_certificate(root))
    }

    fn build(builder: ClientBuilder) -> Result<Self> {
        let client = builder
            .dns_resolver(Arc::new(Guarded))
            .no_proxy()
            .redirect(Policy::none())
            .https_only(true)
            .timeout(TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl Transport for Network {
    async fn send(&self, outbound: Outbound) -> Result<Inbound, Refusal> {
        let host = outbound.host().to_owned();
        let mut request = self
            .client
            .request(outbound.method, outbound.url)
            .headers(outbound.headers);
        if let Some(body) = outbound.body {
            request = request.body(body);
        }
        let response = request
            .send()
            .await
            .map_err(|failed| unreached(&host, &failed))?;
        let status = response.status().as_u16();
        let location =
            header(&response, &LOCATION).and_then(|target| origin_of(response.url(), &target));
        let content_type = header(&response, &CONTENT_TYPE);
        let read = Capped::read(response, RESPONSE_MAX_BYTES)
            .await
            .map_err(|_unread| Refusal::UpstreamUnreachable {
                host,
                reason: NOT_READ,
            })?;
        Ok(Inbound {
            status,
            location,
            content_type,
            body: String::from_utf8(read.bytes)
                .unwrap_or_else(|invalid| String::from_utf8_lossy(invalid.as_bytes()).into_owned()),
            truncated: read.truncated,
        })
    }
}

fn header(response: &Response, name: &HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// The origin a redirect `target` points at, resolved against the request.
fn origin_of(request: &Url, target: &str) -> Option<String> {
    request
        .join(target)
        .ok()
        .map(|resolved| resolved.origin().ascii_serialization())
}

/// The refusal a failed send answers with.
fn unreached(host: &str, failed: &reqwest::Error) -> Refusal {
    let host = host.to_owned();
    if blocked(failed) {
        Refusal::AddressNotAllowed { host }
    } else if failed.is_timeout() {
        Refusal::UpstreamUnreachable {
            host,
            reason: TIMED_OUT,
        }
    } else {
        Refusal::UpstreamUnreachable {
            host,
            reason: NOT_CONNECTED,
        }
    }
}

/// Whether the guard refused the name somewhere in `failed`'s causes.
fn blocked(failed: &(dyn StdError + 'static)) -> bool {
    std::iter::successors(Some(failed), |&error| error.source())
        .any(<dyn StdError>::is::<BlockedAddress>)
}

/// A resolver that refuses a name any of whose addresses is blocked.
#[derive(Debug)]
struct Guarded;

/// The guard's refusal, which `send` reads back out of reqwest's error.
#[derive(Debug, thiserror::Error)]
#[error("the name resolves to an address this runner never reaches")]
struct BlockedAddress;

/// What a resolver hands reqwest when it refuses.
type Unresolved = Box<dyn StdError + Send + Sync>;

impl Resolve for Guarded {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(guarded_lookup(name.as_str().to_owned()))
    }
}

/// Every address `host` resolves to, unless any of them is blocked.
async fn guarded_lookup(host: String) -> Result<Addrs, Unresolved> {
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
    if addresses.iter().any(|address| is_blocked(address.ip())) {
        return Err(Box::new(BlockedAddress));
    }
    Ok(Box::new(addresses.into_iter()))
}

/// A response body read up to a cap.
#[derive(Debug, Default)]
struct Capped {
    bytes: Vec<u8>,
    truncated: bool,
}

impl Capped {
    async fn read(mut response: Response, cap: usize) -> reqwest::Result<Self> {
        let mut capped = Self::default();
        while let Some(chunk) = response.chunk().await? {
            if !capped.push(&chunk, cap) {
                break;
            }
        }
        Ok(capped)
    }

    /// Keeps what of `chunk` fits under `cap`; `false` once the cap is reached.
    fn push(&mut self, chunk: &[u8], cap: usize) -> bool {
        let room = cap.saturating_sub(self.bytes.len());
        let kept = chunk.get(..room).unwrap_or(chunk);
        self.bytes.extend_from_slice(kept);
        self.truncated = kept.len() < chunk.len();
        !self.truncated
    }
}

#[cfg(test)]
#[path = "network/tests.rs"]
mod tests;
