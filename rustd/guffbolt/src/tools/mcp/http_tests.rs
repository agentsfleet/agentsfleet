//! An MCP server over HTTP is never reached at a private address: not when
//! its name resolves to one, not when its hook admits a private literal, and
//! not when its name turns private between the check and the connection.

#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use super::Server;
use crate::tools::code;
use crate::tools::outbound::testing::{Counted, FixedLookup};
use crate::tools::outbound::{
    Admitted, Allowlist, LocalOutbound, Outbound, OutboundFuture, Refused, Request, Response,
};

/// A public address a check may be answered with; nothing is dialled there.
const PUBLIC: &str = "192.0.2.1:443";

/// A hook that admits every URL as written and looks names up through
/// `lookup`, so only the MCP client's own guard stands between a server and
/// a private address.
struct AdmitAll {
    lookup: Arc<dyn Resolve>,
}

impl std::fmt::Debug for AdmitAll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmitAll").finish_non_exhaustive()
    }
}

impl Outbound for AdmitAll {
    fn send<'a>(&'a self, _tool: &'a str, _request: Request) -> OutboundFuture<'a, Response> {
        Box::pin(std::future::ready(Err(Refused::new(
            code::HOST_NOT_ALLOWED,
            "this hook only admits connections",
        ))))
    }

    fn mask(&self, text: String) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(std::future::ready(text))
    }

    fn secret<'a>(
        &'a self,
        _tool: &'a str,
        name: &'a str,
        _field: &'a str,
    ) -> OutboundFuture<'a, String> {
        let missing = Refused::new(code::SECRET_NOT_FOUND, format!("no secret {name}"));
        Box::pin(std::future::ready(Err(missing)))
    }

    fn connect<'a>(&'a self, _tool: &'a str, url: &'a str) -> OutboundFuture<'a, Admitted> {
        Box::pin(std::future::ready(Ok(Admitted {
            url: url.to_owned(),
            headers: Vec::new(),
        })))
    }

    fn lookup(&self) -> Arc<dyn Resolve> {
        Arc::clone(&self.lookup)
    }
}

/// A name answered with a public address the first time it is looked up and
/// a private one every time after: an answer that changed after it was
/// checked.
#[derive(Debug)]
struct Rebinding {
    first: SocketAddr,
    after: SocketAddr,
    asked: AtomicUsize,
}

impl Resolve for Rebinding {
    fn resolve(&self, _name: Name) -> Resolving {
        let address = if self.asked.fetch_add(1, Ordering::SeqCst) == 0 {
            self.first
        } else {
            self.after
        };
        Box::pin(std::future::ready(Ok(
            Box::new(std::iter::once(address)) as Addrs
        )))
    }
}

/// The refusal a connection reads, or `None` when it connected.
async fn refusal(url: &str, outbound: &dyn Outbound) -> Option<String> {
    Server::http("docs", url, outbound)
        .await
        .err()
        .map(|failure| failure.to_string())
}

#[tokio::test]
async fn mcp_http_client_refuses_a_private_address() {
    let private: SocketAddr = "10.0.0.1:443".parse().expect("an address");
    let lookup = Arc::new(FixedLookup::default().answering("mcp.test", &[private]));
    let outbound = LocalOutbound::resolving(Allowlist::Any, Arc::<FixedLookup>::clone(&lookup))
        .expect("the client builds");

    let refused = refusal("https://mcp.test/mcp", &outbound).await;

    assert_eq!(
        refused.as_deref(),
        Some("[egress_denied] mcp.test resolves to a private, loopback or reserved address")
    );
    assert_eq!(
        lookup.asked(),
        ["mcp.test"],
        "looked up once, by the check; the client never looked it up to connect"
    );
}

#[tokio::test]
async fn mcp_http_client_refuses_a_private_literal_its_hook_admits() {
    let lookup = Arc::new(FixedLookup::default());
    let outbound = AdmitAll {
        lookup: Arc::<FixedLookup>::clone(&lookup),
    };

    let refused = refusal("https://[fd00::1]:8443/mcp", &outbound).await;

    assert_eq!(
        refused.as_deref(),
        Some("[egress_denied] [fd00::1] is a private, loopback or reserved address")
    );
    assert_eq!(lookup.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn mcp_http_client_refuses_a_name_that_turns_private_after_admission() {
    let inside = Counted::listen().await;
    let lookup = Arc::new(Rebinding {
        first: PUBLIC.parse().expect("an address"),
        after: inside.address,
        asked: AtomicUsize::new(0),
    });
    let outbound = AdmitAll {
        lookup: Arc::<Rebinding>::clone(&lookup),
    };

    let refused = refusal("https://rebinding.test/mcp", &outbound).await;

    assert!(refused.is_some(), "the connection was refused");
    assert_eq!(
        lookup.asked.load(Ordering::SeqCst),
        2,
        "the check, then the client"
    );
    assert_eq!(
        inside.connections(),
        0,
        "the private address got no connection"
    );
}
