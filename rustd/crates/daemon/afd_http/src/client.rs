//! Who the HTTP caller actually is, behind whatever proxy is in front of us.
//!
//! Every non-development deployment sits behind Fly's proxy, so the socket's
//! peer is Fly and not the person. `Fly-Client-IP` is the trust anchor: the
//! proxy stamps it on every request and strips a client-supplied copy. Off Fly
//! the rightmost `X-Forwarded-For` hop stands, the one the nearest proxy wrote,
//! and a direct-connected caller is its socket peer.
//!
//! The extraction is `axum-client-ip`'s. What stays here is the fallback
//! across its three sources, because each of its extractors refuses with a 500
//! when its own source is absent, and a request with no proxy in front of it is
//! a caller this daemon serves rather than refuses.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum_client_ip::{FlyClientIp, RightmostXForwardedFor};
use http::request::Parts;

/// What a request reports when no header and no socket peer name an address.
///
/// Reached only where the connection info extension was never inserted and no
/// proxy header arrived; a placeholder in a digest, never a decision anything
/// branches on.
const ORIGIN_UNKNOWN: &str = "unknown";

/// What a request reports when it names no user agent.
const USER_AGENT_UNKNOWN: &str = "unknown";

/// The caller's address and user agent, as a parameter a handler declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// Where the request came from, after the proxy is accounted for.
    pub address: String,
    /// What the caller called itself, or `unknown`.
    pub user_agent: String,
}

impl<S: Send + Sync> FromRequestParts<S> for Origin {
    /// Infallible: every field has an answer for an absent input.
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let address = address(parts, state)
            .await
            .map_or_else(|| ORIGIN_UNKNOWN.to_owned(), |ip| ip.to_string());
        let user_agent = parts
            .headers
            .get(http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .unwrap_or(USER_AGENT_UNKNOWN)
            .to_owned();
        Ok(Self {
            address,
            user_agent,
        })
    }
}

/// The proxy's header, then the forwarded chain, then the socket peer.
async fn address<S: Send + Sync>(parts: &mut Parts, state: &S) -> Option<IpAddr> {
    if let Ok(FlyClientIp(ip)) = FlyClientIp::from_request_parts(parts, state).await {
        return Some(ip);
    }
    if let Ok(RightmostXForwardedFor(ip)) =
        RightmostXForwardedFor::from_request_parts(parts, state).await
    {
        return Some(ip);
    }
    parts
        .extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(peer)| peer.ip())
}

#[cfg(test)]
mod tests {
    use super::Origin;
    use axum::extract::FromRequestParts as _;

    async fn origin(headers: &[(&str, &str)]) -> Origin {
        let mut request = http::Request::builder();
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let (mut parts, ()) = request.body(()).unwrap_or_default().into_parts();
        let Ok(origin) = Origin::from_request_parts(&mut parts, &()).await;
        origin
    }

    /// The proxy's header beats a forged chain; without it the nearest hop
    /// stands, and without either the request still answers.
    #[tokio::test]
    async fn the_fly_header_wins_over_the_forwarded_chain() {
        let forged = origin(&[
            ("x-forwarded-for", "198.51.100.9"),
            ("fly-client-ip", "203.0.113.7"),
        ])
        .await;
        assert_eq!(forged.address, "203.0.113.7");

        let chained = origin(&[("x-forwarded-for", "198.51.100.9, 203.0.113.7")]).await;
        assert_eq!(chained.address, "203.0.113.7", "the rightmost hop");

        let bare = origin(&[]).await;
        assert_eq!(
            (bare.address.as_str(), bare.user_agent.as_str()),
            ("unknown", "unknown")
        );
    }
}
