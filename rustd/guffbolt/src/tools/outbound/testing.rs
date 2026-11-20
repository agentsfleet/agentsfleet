//! Stand-ins for the network a guard test reaches: a lookup with fixed
//! answers that records every name it is asked, and a listener that counts
//! the connections made to it.

#![expect(
    clippy::expect_used,
    reason = "test fixture: a failed precondition should fail the test loudly"
)]

use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use tokio::net::TcpListener;

/// A lookup that answers each name it holds with fixed addresses, refuses
/// any other, and records every name it was asked, in order.
#[derive(Debug, Default)]
pub(crate) struct FixedLookup {
    answers: BTreeMap<String, Vec<SocketAddr>>,
    asked: Mutex<Vec<String>>,
}

impl FixedLookup {
    /// The same, answering `name` with `addresses`.
    pub(crate) fn answering(mut self, name: &str, addresses: &[SocketAddr]) -> Self {
        self.answers.insert(name.to_owned(), addresses.to_vec());
        self
    }

    /// Every name it was asked, in order.
    pub(crate) fn asked(&self) -> Vec<String> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Resolve for FixedLookup {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(host.clone());
        let answer: Result<Vec<SocketAddr>, Box<dyn std::error::Error + Send + Sync>> =
            self.answers.get(&host).cloned().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("no answer for {host}")).into()
            });
        Box::pin(std::future::ready(
            answer.map(|addresses| Box::new(addresses.into_iter()) as Addrs),
        ))
    }
}

/// A listener on loopback that accepts each connection, counts it and drops
/// it. The count rises before the drop, so a client the drop fails has been
/// counted by the time it sees the failure.
#[derive(Debug)]
pub(crate) struct Counted {
    /// Where it listens.
    pub(crate) address: SocketAddr,
    connections: Arc<AtomicUsize>,
}

impl Counted {
    pub(crate) async fn listen() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback takes a listener");
        let address = listener
            .local_addr()
            .expect("a bound listener has an address");
        let connections = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&connections);
        tokio::spawn(async move {
            while let Ok((socket, _peer)) = listener.accept().await {
                counted.fetch_add(1, Ordering::SeqCst);
                drop(socket);
            }
        });
        Self {
            address,
            connections,
        }
    }

    /// How many connections reached it.
    pub(crate) fn connections(&self) -> usize {
        self.connections.load(Ordering::SeqCst)
    }
}
