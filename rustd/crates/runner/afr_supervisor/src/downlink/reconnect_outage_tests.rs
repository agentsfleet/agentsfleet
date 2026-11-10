//! A runner reaches a daemon that returns after an outage over HTTP/3, and skips HTTP/3 for 30
//! seconds only after HTTP/3 failed and HTTP/2 carried.

use super::Preference;
use crate::{
    config::StreamConfig, error::ErrorKind, test_support::healthy, test_util::NativeDaemon,
};
use af_stream::{
    Connection, Endpoint, Incoming, Listener, Outgoing, Reason, Transport,
    test_util::IdentityFixture, tls::Trust,
};
use http::{Request, Response};
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, UdpSocket},
    time::Instant,
};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

type Proof<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// A daemon that answers on both carriers and counts the connections it sees on each.
struct Daemon {
    config: StreamConfig,
    http3: Arc<AtomicUsize>,
    http2: Arc<AtomicUsize>,
    _stop: tokio_util::sync::DropGuard,
    _server: AbortOnDropHandle<af_stream::Result<()>>,
}

impl Daemon {
    async fn start(identity: &IdentityFixture) -> Proof<Self> {
        let listener = Listener::bind("[::1]:0".parse()?, identity.identity()).await?;
        let endpoint = Endpoint::parse(&format!(
            "https://localhost:{}",
            listener.local_addr()?.port()
        ))?;
        let stop = CancellationToken::new();
        let http3 = Arc::new(AtomicUsize::new(0));
        let http2 = Arc::new(AtomicUsize::new(0));
        let (counted3, counted2) = (Arc::clone(&http3), Arc::clone(&http2));
        let server = AbortOnDropHandle::new(tokio::spawn(listener.serve(
            move |info| {
                if info.transport == Transport::H3 {
                    counted3.fetch_add(1, Ordering::SeqCst);
                } else {
                    counted2.fetch_add(1, Ordering::SeqCst);
                }
                tower::service_fn(|request: Request<Incoming>| async move {
                    Ok::<_, Infallible>(Response::new(Outgoing::new(request.into_body())))
                })
            },
            stop.clone(),
            stop.clone(),
        )));
        Ok(Self {
            config: StreamConfig {
                endpoint,
                trust: identity.trust(),
            },
            http3,
            http2,
            _stop: stop.drop_guard(),
            _server: server,
        })
    }

    fn assert_saw(&self, http3: usize, http2: usize) {
        assert_eq!(
            self.http3.load(Ordering::SeqCst),
            http3,
            "HTTP/3 connections the daemon saw"
        );
        assert_eq!(
            self.http2.load(Ordering::SeqCst),
            http2,
            "HTTP/2 connections the daemon saw"
        );
    }
}

/// Why an attempt failed on a native carrier; `None` when it connected or failed otherwise.
fn failure_reason(attempt: crate::Result<Connection>) -> Option<Reason> {
    match attempt {
        Err(error) => match error.kind() {
            ErrorKind::Stream { source } => Some(source.reason()),
            _ => None,
        },
        Ok(_) => None,
    }
}

/// The daemon restarts in about ten seconds, and the runner's attempts during that time fail on
/// both carriers. The first connection after it must still prefer HTTP/3: a healthy HTTP/2 link
/// is never interrupted, so whatever carries the first success carries the session.
///
/// A deadline that has already passed fails both carriers before either reaches the daemon.
#[tokio::test]
async fn a_daemon_that_returns_after_an_outage_is_reached_over_http3() -> Proof {
    let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
    let daemon = Daemon::start(&identity).await?;
    let stop = CancellationToken::new();
    let mut preference = Preference::default();
    let outage = preference
        .connect(&daemon.config, &stop, Instant::now())
        .await;
    assert_eq!(
        failure_reason(outage),
        Some(Reason::Timeout),
        "an attempt with no time left must fail on a deadline"
    );
    let connection = preference
        .connect(
            &daemon.config,
            &stop,
            Instant::now() + Duration::from_secs(5),
        )
        .await?;
    assert_eq!(connection.transport(), Transport::H3);
    daemon.assert_saw(1, 0);
    Ok(())
}

/// The same outage with sockets that exist and never answer on either carrier, so both attempts
/// run their full connect timeouts as they do against a daemon that is down.
#[tokio::test]
async fn a_daemon_that_returns_after_an_unanswered_outage_is_reached_over_http3() -> Proof {
    let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
    let daemon = Daemon::start(&identity).await?;
    let silent_tcp = TcpListener::bind("[::1]:0").await?;
    let address = silent_tcp.local_addr()?;
    let _silent_udp = UdpSocket::bind(address).await?;
    let down = StreamConfig {
        endpoint: Endpoint::parse(&format!("https://localhost:{}", address.port()))?,
        trust: identity.trust(),
    };
    let stop = CancellationToken::new();
    let mut preference = Preference::default();
    let outage = preference
        .connect(&down, &stop, Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(
        failure_reason(outage),
        Some(Reason::Timeout),
        "an endpoint that never answers must fail on a deadline"
    );
    let connection = preference
        .connect(
            &daemon.config,
            &stop,
            Instant::now() + Duration::from_secs(5),
        )
        .await?;
    assert_eq!(connection.transport(), Transport::H3);
    daemon.assert_saw(1, 0);
    Ok(())
}

/// A port nobody listens on refuses the connection, which is a network failure rather than a
/// deadline: a rule that armed the skip for one of the two reasons would still pass the other
/// outage tests.
#[tokio::test]
async fn a_daemon_that_returns_after_a_refused_outage_is_reached_over_http3() -> Proof {
    let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
    let daemon = Daemon::start(&identity).await?;
    let closed = TcpListener::bind("[::1]:0").await?.local_addr()?;
    let down = StreamConfig {
        endpoint: Endpoint::parse(&format!("https://localhost:{}", closed.port()))?,
        trust: identity.trust(),
    };
    let stop = CancellationToken::new();
    let mut preference = Preference::default();
    let outage = preference
        .connect(&down, &stop, Instant::now() + Duration::from_secs(5))
        .await;
    assert_eq!(
        failure_reason(outage),
        Some(Reason::Network),
        "a port nobody listens on must fail as a network error"
    );
    let connection = preference
        .connect(
            &daemon.config,
            &stop,
            Instant::now() + Duration::from_secs(5),
        )
        .await?;
    assert_eq!(connection.transport(), Transport::H3);
    daemon.assert_saw(1, 0);
    Ok(())
}

/// UDP unanswered, TCP forwarded: the first connect tries HTTP/3 and lands on HTTP/2; the second,
/// inside the window, goes straight to HTTP/2 and sends no datagram at all.
#[tokio::test]
async fn a_connect_after_http3_failed_and_http2_carried_skips_http3_without_a_datagram() -> Proof {
    let mut daemon = NativeDaemon::serve_idle(healthy(), |_frame| async {}).await?;
    let relay = daemon.tcp_only().await?;
    let config = StreamConfig {
        endpoint: Endpoint::parse(&relay.url)?,
        trust: Trust::with_roots(daemon.root()),
    };
    let stop = CancellationToken::new();
    let mut preference = Preference::default();
    let first = preference
        .connect(&config, &stop, Instant::now() + Duration::from_secs(10))
        .await?;
    assert_eq!(
        first.transport(),
        Transport::H2,
        "UDP is unanswered, so HTTP/2 must carry"
    );
    assert!(
        relay.udp_datagrams() > 0,
        "the first connect never tried HTTP/3"
    );
    let second = preference
        .connect(&config, &stop, Instant::now() + Duration::from_secs(10))
        .await?;
    assert_eq!(second.transport(), Transport::H2);
    assert_eq!(
        relay.udp_datagrams(),
        0,
        "the second connect sent HTTP/3 datagrams inside the window"
    );
    drop((first, second));
    relay.close().await?;
    daemon.close().await?;
    Ok(())
}
