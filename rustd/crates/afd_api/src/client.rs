//! Who the caller actually is, behind whatever proxy is in front of us.
//!
//! # The raw peer is the load balancer
//!
//! Every non-development deployment sits behind Fly's proxy, so the socket's
//! peer address is Fly and not the person. Two headers carry the real one, and
//! they are trusted differently:
//!
//! - `X-Forwarded-For` is the industry-standard chain, leftmost-first, and
//!   anybody can send it.
//! - `Fly-Client-IP` is stamped by the proxy on every request it forwards, and
//!   the proxy STRIPS a client-supplied copy of its own header. That is what
//!   makes it the trust anchor rather than a second opinion.
//!
//! The default is still the forwarded-for chain, because that is what an
//! operator who has seen any HTTP infrastructure expects an audit trail to
//! read like. When both are present and DISAGREE, the proxy's view wins and
//! the divergence is recorded — that pair is the signature of somebody trying
//! to forge an origin, and it is worth being able to grep for.
//!
//! There is deliberately no trusted-proxy allowlist. The Fly header IS the
//! anchor; an allowlist would be a second, weaker mechanism to keep in step
//! with a network topology this daemon does not own.

use axum::extract::FromRequestParts;
use http::request::Parts;
use std::convert::Infallible;
use std::net::SocketAddr;

/// The forwarded chain, leftmost entry first.
const HEADER_FORWARDED_FOR: &str = "x-forwarded-for";

/// The proxy's own authoritative single-value header.
const HEADER_FLY_CLIENT_IP: &str = "fly-client-ip";

/// What a request reports when no header and no socket peer are available.
///
/// Reached only where the connection info extension was never inserted, which
/// is a wiring fault rather than a caller's doing — the value is a placeholder
/// in an audit field, never a decision anything branches on.
pub const ORIGIN_UNKNOWN: &str = "unknown";

/// What a request reports when it names no user agent.
pub const USER_AGENT_UNKNOWN: &str = "unknown";

/// Which signal the effective address came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSource {
    /// The forwarded chain, which agreed with the proxy or stood alone.
    ForwardedFor,
    /// The proxy's header, either alone or because the two disagreed.
    ProxyHeader,
    /// Neither header was usable, so the socket peer stands.
    SocketPeer,
}

/// The caller's effective address, and how it was decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAddress {
    address: String,
    source: AddressSource,
    divergent: bool,
}

impl ClientAddress {
    /// The address downstream code attributes the request to.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.address
    }

    /// Which signal it came from, for the audit field.
    #[must_use]
    pub const fn source(&self) -> AddressSource {
        self.source
    }

    /// Whether the two headers disagreed, which is the forgery signature.
    #[must_use]
    pub const fn is_divergent(&self) -> bool {
        self.divergent
    }

    /// Decides the effective address from the peer and the two headers.
    ///
    /// Pure, and taking three optional strings rather than a request, so the
    /// whole trust model is exercised without one — the property the Zig
    /// `deriveClientIp` split itself out of its middleware to get.
    #[must_use]
    pub fn derive(peer: Option<&str>, forwarded_for: Option<&str>, proxy: Option<&str>) -> Self {
        let chain = forwarded_for.and_then(leftmost_entry);
        let anchor = proxy.and_then(trimmed);

        match (chain, anchor) {
            // Both present and equal: the ordinary proxied request.
            (Some(chain), Some(anchor)) if chain == anchor => {
                Self::of(chain, AddressSource::ForwardedFor, false)
            }
            // Both present and different: the proxy is right, and somebody was
            // trying to be somewhere else.
            (Some(_), Some(anchor)) => Self::of(anchor, AddressSource::ProxyHeader, true),
            (Some(chain), None) => Self::of(chain, AddressSource::ForwardedFor, false),
            (None, Some(anchor)) => Self::of(anchor, AddressSource::ProxyHeader, false),
            (None, None) => Self::of(
                peer.and_then(trimmed).unwrap_or(ORIGIN_UNKNOWN),
                AddressSource::SocketPeer,
                false,
            ),
        }
    }

    fn of(address: &str, source: AddressSource, divergent: bool) -> Self {
        Self {
            address: address.to_owned(),
            source,
            divergent,
        }
    }
}

/// The caller's address and user agent, as a parameter a handler declares.
///
/// An extractor rather than four lines at the top of each handler, which is the
/// shape `helpers.buildScratch` is reaching for and cannot have: there, every
/// device-flow handler declares `var scratch: RequestScratch = undefined` and
/// fills it on the next line, and the compiler cannot tell the ones that forgot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Where the request came from, after the proxy is accounted for.
    pub address: ClientAddress,
    /// What the caller called itself, or [`USER_AGENT_UNKNOWN`].
    pub user_agent: String,
}

impl<S: Send + Sync> FromRequestParts<S> for Origin {
    /// Infallible: every field has a documented answer for an absent input, and
    /// a request with no headers at all is a request from a direct-connected
    /// client rather than a request this daemon should refuse.
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send {
        let header = |name: &str| {
            parts
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        // The socket peer, where the accept loop recorded one. Only the address
        // half: the ephemeral port changes per connection, so including it would
        // give one caller a different fingerprint on every reconnect.
        let peer = parts
            .extensions
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip().to_string());
        let forwarded_for = header(HEADER_FORWARDED_FOR);
        let proxy = header(HEADER_FLY_CLIENT_IP);
        let user_agent = header(http::header::USER_AGENT.as_str())
            .unwrap_or_else(|| USER_AGENT_UNKNOWN.to_owned());

        std::future::ready(Ok(Self {
            address: ClientAddress::derive(
                peer.as_deref(),
                forwarded_for.as_deref(),
                proxy.as_deref(),
            ),
            user_agent,
        }))
    }
}

/// The leftmost non-empty entry of a forwarded chain.
fn leftmost_entry(header: &str) -> Option<&str> {
    header
        .split(',')
        .map(str::trim)
        .find(|entry| !entry.is_empty())
}

/// A single-value header, or `None` when it is absent or only whitespace.
fn trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_socket_peer_stands_when_no_header_carries_one() {
        let derived = ClientAddress::derive(Some("203.0.113.7"), None, None);
        assert_eq!(derived.as_str(), "203.0.113.7");
        assert_eq!(derived.source(), AddressSource::SocketPeer);
        assert!(!derived.is_divergent());
    }

    #[test]
    fn the_forwarded_chain_wins_when_it_stands_alone() {
        let derived = ClientAddress::derive(Some("10.0.0.1"), Some("203.0.113.7, 10.0.0.1"), None);
        assert_eq!(derived.as_str(), "203.0.113.7");
        assert_eq!(derived.source(), AddressSource::ForwardedFor);
    }

    #[test]
    fn the_proxy_header_wins_and_records_the_disagreement() {
        let derived =
            ClientAddress::derive(Some("10.0.0.1"), Some("198.51.100.9"), Some("203.0.113.7"));
        assert_eq!(derived.as_str(), "203.0.113.7");
        assert_eq!(derived.source(), AddressSource::ProxyHeader);
        assert!(derived.is_divergent(), "a forged chain must be greppable");
    }

    #[test]
    fn agreement_reads_as_the_forwarded_chain_and_is_not_divergent() {
        let derived =
            ClientAddress::derive(Some("10.0.0.1"), Some("203.0.113.7"), Some("203.0.113.7"));
        assert_eq!(derived.source(), AddressSource::ForwardedFor);
        assert!(!derived.is_divergent());
    }

    #[test]
    fn a_whitespace_only_or_comma_only_chain_reads_as_absent() {
        for chain in ["", "   ", ",", " , , "] {
            let derived = ClientAddress::derive(Some("10.0.0.1"), Some(chain), None);
            assert_eq!(derived.as_str(), "10.0.0.1", "chain {chain:?}");
            assert_eq!(derived.source(), AddressSource::SocketPeer);
        }
    }

    #[test]
    fn a_request_with_nothing_at_all_still_answers() {
        let derived = ClientAddress::derive(None, None, None);
        assert_eq!(derived.as_str(), ORIGIN_UNKNOWN);
        assert_eq!(derived.source(), AddressSource::SocketPeer);
    }
}
