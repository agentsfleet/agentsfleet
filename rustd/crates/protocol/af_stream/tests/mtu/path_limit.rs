//! A UDP relay that stands in for a network path whose MTU is smaller than loopback's.
//!
//! Loopback carries 64 KiB datagrams, so a QUIC endpoint that sends datagrams the real path cannot
//! carry looks healthy in every other suite. The relay forwards between one client and the server,
//! counts and sizes what each side sends, drops what a path of its limit drops, and can lose a
//! deterministic share of the large datagrams that remain. Every drop is silent: neither endpoint
//! can read why a datagram never arrived.

use std::{
    net::{Ipv6Addr, SocketAddr},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_stream::Endpoint;
use tokio::net::UdpSocket;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use crate::support::{Host, Result};

const DATAGRAM_MAX: usize = 65_536;
/// Only datagrams larger than this are eligible for deterministic loss; acknowledgements stay.
const LOSS_FLOOR_BYTES: usize = 1_000;
const UNLIMITED: usize = usize::MAX;

/// How the path treats datagrams.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Path {
    /// Largest UDP payload carried toward the client; anything larger vanishes.
    pub(crate) limit_to_client: Option<usize>,
    /// Largest UDP payload carried toward the server; anything larger vanishes.
    pub(crate) limit_to_server: Option<usize>,
    /// Lose every Nth eligible datagram in each direction; `None` loses none.
    pub(crate) lose_every: Option<NonZeroUsize>,
}

impl Path {
    pub(crate) const fn unconstrained() -> Self {
        Self {
            limit_to_client: None,
            limit_to_server: None,
            lose_every: None,
        }
    }

    /// The same limit both ways.
    pub(crate) const fn limited(bytes: usize) -> Self {
        Self {
            limit_to_client: Some(bytes),
            limit_to_server: Some(bytes),
            lose_every: None,
        }
    }

    pub(crate) const fn lossy(bytes: usize, every: NonZeroUsize) -> Self {
        Self {
            lose_every: Some(every),
            ..Self::limited(bytes)
        }
    }
}

/// What crossed the relay, by direction.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Traffic {
    pub(crate) largest_to_client: usize,
    pub(crate) largest_to_server: usize,
    pub(crate) datagrams_to_client: usize,
    pub(crate) datagrams_to_server: usize,
    pub(crate) dropped_to_client: usize,
    pub(crate) dropped_to_server: usize,
    pub(crate) lost: usize,
}

struct Shared {
    limit_to_client: AtomicUsize,
    limit_to_server: AtomicUsize,
    largest_to_client: AtomicUsize,
    largest_to_server: AtomicUsize,
    datagrams_to_client: AtomicUsize,
    datagrams_to_server: AtomicUsize,
    dropped_to_client: AtomicUsize,
    dropped_to_server: AtomicUsize,
    lost: AtomicUsize,
}

pub(crate) struct PathLimit {
    /// Where the client connects; the relay forwards to the host's own UDP port.
    pub(crate) endpoint: Endpoint,
    shared: Arc<Shared>,
    _task: AbortOnDropHandle<std::io::Result<()>>,
}

impl PathLimit {
    /// Relay UDP to `host`. Bind every listener before the first relay, so a relay's ephemeral
    /// UDP port cannot be the number a later listener's TCP socket was given.
    pub(crate) async fn start(host: &Host, path: Path) -> Result<Self> {
        let front = UdpSocket::bind("[::1]:0").await?;
        let back = UdpSocket::bind("[::1]:0").await?;
        let port = host
            .endpoint
            .uri()
            .port_u16()
            .ok_or("backend port missing")?;
        back.connect(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
            .await?;
        let endpoint =
            Endpoint::parse(&format!("https://localhost:{}", front.local_addr()?.port()))?;
        let shared = Arc::new(Shared {
            limit_to_client: AtomicUsize::new(path.limit_to_client.unwrap_or(UNLIMITED)),
            limit_to_server: AtomicUsize::new(path.limit_to_server.unwrap_or(UNLIMITED)),
            largest_to_client: AtomicUsize::new(0),
            largest_to_server: AtomicUsize::new(0),
            datagrams_to_client: AtomicUsize::new(0),
            datagrams_to_server: AtomicUsize::new(0),
            dropped_to_client: AtomicUsize::new(0),
            dropped_to_server: AtomicUsize::new(0),
            lost: AtomicUsize::new(0),
        });
        let task = tokio::spawn(relay(
            front,
            back,
            path.lose_every,
            Arc::clone(&shared),
            host.stop.child_token(),
        ));
        Ok(Self {
            endpoint,
            shared,
            _task: AbortOnDropHandle::new(task),
        })
    }

    /// Change the limit in both directions while a connection is open: the path shrank.
    pub(crate) fn set_limit(&self, bytes: usize) {
        self.shared.limit_to_client.store(bytes, Ordering::SeqCst);
        self.shared.limit_to_server.store(bytes, Ordering::SeqCst);
    }

    pub(crate) fn traffic(&self) -> Traffic {
        let shared = &self.shared;
        Traffic {
            largest_to_client: shared.largest_to_client.load(Ordering::SeqCst),
            largest_to_server: shared.largest_to_server.load(Ordering::SeqCst),
            datagrams_to_client: shared.datagrams_to_client.load(Ordering::SeqCst),
            datagrams_to_server: shared.datagrams_to_server.load(Ordering::SeqCst),
            dropped_to_client: shared.dropped_to_client.load(Ordering::SeqCst),
            dropped_to_server: shared.dropped_to_server.load(Ordering::SeqCst),
            lost: shared.lost.load(Ordering::SeqCst),
        }
    }
}

enum Verdict {
    Forward,
    TooBig,
    Lost,
}

/// Decide one datagram: over the limit it vanishes; otherwise every Nth eligible one is lost.
fn judge(
    size: usize,
    limit: usize,
    eligible_seen: &mut usize,
    lose_every: Option<NonZeroUsize>,
) -> Verdict {
    if size > limit {
        return Verdict::TooBig;
    }
    if let Some(every) = lose_every
        && size > LOSS_FLOOR_BYTES
    {
        *eligible_seen += 1;
        if eligible_seen.is_multiple_of(every.get()) {
            return Verdict::Lost;
        }
    }
    Verdict::Forward
}

async fn relay(
    front: UdpSocket,
    back: UdpSocket,
    lose_every: Option<NonZeroUsize>,
    shared: Arc<Shared>,
    stop: CancellationToken,
) -> std::io::Result<()> {
    let mut from_client = vec![0_u8; DATAGRAM_MAX];
    let mut from_server = vec![0_u8; DATAGRAM_MAX];
    let mut client: Option<SocketAddr> = None;
    let (mut eligible_to_server, mut eligible_to_client) = (0_usize, 0_usize);
    loop {
        tokio::select! {
            () = stop.cancelled() => return Ok(()),
            received = front.recv_from(&mut from_client) => {
                let (size, address) = received?;
                let datagram = from_client.get(..size).ok_or(std::io::ErrorKind::InvalidData)?;
                client = Some(address);
                shared.largest_to_server.fetch_max(size, Ordering::SeqCst);
                shared.datagrams_to_server.fetch_add(1, Ordering::SeqCst);
                let limit = shared.limit_to_server.load(Ordering::SeqCst);
                match judge(size, limit, &mut eligible_to_server, lose_every) {
                    Verdict::TooBig => { shared.dropped_to_server.fetch_add(1, Ordering::SeqCst); }
                    Verdict::Lost => { shared.lost.fetch_add(1, Ordering::SeqCst); }
                    Verdict::Forward => { back.send(datagram).await?; }
                }
            }
            received = back.recv(&mut from_server) => {
                let size = received?;
                let datagram = from_server.get(..size).ok_or(std::io::ErrorKind::InvalidData)?;
                shared.largest_to_client.fetch_max(size, Ordering::SeqCst);
                shared.datagrams_to_client.fetch_add(1, Ordering::SeqCst);
                let limit = shared.limit_to_client.load(Ordering::SeqCst);
                match judge(size, limit, &mut eligible_to_client, lose_every) {
                    Verdict::TooBig => { shared.dropped_to_client.fetch_add(1, Ordering::SeqCst); }
                    Verdict::Lost => { shared.lost.fetch_add(1, Ordering::SeqCst); }
                    Verdict::Forward => {
                        if let Some(client) = client {
                            front.send_to(datagram, client).await?;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn a_datagram_over_the_limit_is_dropped_and_one_at_the_limit_is_carried() {
    let mut seen = 0;
    assert!(matches!(
        judge(1_233, 1_232, &mut seen, None),
        Verdict::TooBig
    ));
    assert!(matches!(
        judge(1_232, 1_232, &mut seen, None),
        Verdict::Forward
    ));
}

#[test]
fn loss_takes_every_nth_large_datagram_and_never_a_small_one() {
    let mut seen = 0;
    let every = NonZeroUsize::new(3);
    let large: Vec<bool> = (0..6)
        .map(|_| matches!(judge(1_200, UNLIMITED, &mut seen, every), Verdict::Lost))
        .collect();
    assert_eq!(large, [false, false, true, false, false, true]);
    let small = (0..6).all(|_| {
        matches!(
            judge(LOSS_FLOOR_BYTES, UNLIMITED, &mut seen, every),
            Verdict::Forward
        )
    });
    assert!(small, "a datagram at the loss floor was lost");
}
