//! Literal origins verify IP subject alternative names on both native carriers.

use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_stream::{Connection, Endpoint, Incoming, Listener, Outgoing, Reason, Transport};
use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Empty;
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

use super::support::{Fixture, Result, deadline};

#[derive(Default)]
struct Census {
    http3: AtomicUsize,
    http2: AtomicUsize,
    requests: AtomicUsize,
}

impl Census {
    fn connected(&self, transport: Transport) {
        match transport {
            Transport::H3 => self.http3.fetch_add(1, Ordering::SeqCst),
            Transport::H2 => self.http2.fetch_add(1, Ordering::SeqCst),
        };
    }

    fn connections(&self) -> (usize, usize) {
        (
            self.http3.load(Ordering::SeqCst),
            self.http2.load(Ordering::SeqCst),
        )
    }
}

struct Host {
    endpoint: Endpoint,
    census: Arc<Census>,
    stop: CancellationToken,
    task: AbortOnDropHandle<af_stream::Result<()>>,
}

impl Host {
    async fn serve(fixture: &Fixture, ip: IpAddr) -> Result<Self> {
        let listener = Listener::bind(SocketAddr::new(ip, 0), fixture.identity.clone()).await?;
        let endpoint = Endpoint::parse(&format!("https://{}", listener.local_addr()?))?;
        let census = Arc::new(Census::default());
        let counted = Arc::clone(&census);
        let stop = CancellationToken::new();
        let task = tokio::spawn(listener.serve(
            move |info| {
                counted.connected(info.transport);
                let census = Arc::clone(&counted);
                tower::service_fn(move |_request: Request<Incoming>| {
                    census.requests.fetch_add(1, Ordering::SeqCst);
                    async {
                        Ok::<_, Infallible>(Response::new(Outgoing::new(Empty::<Bytes>::new())))
                    }
                })
            },
            stop.clone(),
            stop.clone(),
        ));
        Ok(Self {
            endpoint,
            census,
            stop,
            task: AbortOnDropHandle::new(task),
        })
    }

    async fn close(mut self) -> Result {
        self.stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), &mut self.task).await???;
        Ok(())
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[tokio::test]
async fn both_transports_verify_an_ipv4_subject_alternative_name() -> Result {
    matching(Ipv4Addr::LOCALHOST.into()).await
}

#[tokio::test]
async fn both_transports_verify_an_ipv6_subject_alternative_name() -> Result {
    matching(Ipv6Addr::LOCALHOST.into()).await
}

async fn matching(ip: IpAddr) -> Result {
    let fixture = Fixture::new(vec![ip.to_string()], false)?;
    let host = Host::serve(&fixture, ip).await?;
    for transport in [Transport::H3, Transport::H2] {
        let connection = Connection::connect(
            transport,
            &host.endpoint,
            &fixture.trust,
            &host.stop,
            deadline(),
        )
        .await?;
        assert_eq!(
            connection.key_exchange(),
            Some(af_stream::tls::KeyExchange::X25519MlKem768)
        );
        connection.close(deadline()).await?;
    }
    assert_eq!(host.census.connections(), (1, 1));
    host.close().await
}

#[tokio::test]
async fn a_wrong_ipv4_subject_alternative_name_is_refused_without_fallback() -> Result {
    mismatched(Ipv4Addr::LOCALHOST.into(), "127.0.0.2").await
}

#[tokio::test]
async fn a_wrong_ipv6_subject_alternative_name_is_refused_without_fallback() -> Result {
    mismatched(Ipv6Addr::LOCALHOST.into(), "::2").await
}

async fn mismatched(ip: IpAddr, certificate_ip: &str) -> Result {
    let fixture = Fixture::new(vec![certificate_ip.to_owned()], false)?;
    let host = Host::serve(&fixture, ip).await?;
    for transport in [Transport::H3, Transport::H2] {
        let result = Connection::connect(
            transport,
            &host.endpoint,
            &fixture.trust,
            &host.stop,
            deadline(),
        )
        .await;
        let error = result
            .err()
            .ok_or("a certificate for another IP was accepted")?;
        assert_eq!(error.reason(), Reason::Tls);
    }
    assert_eq!(host.census.connections(), (1, 1));
    let result =
        Connection::preferred(&host.endpoint, &fixture.trust, &host.stop, deadline()).await;
    let error = result
        .err()
        .ok_or("an IP identity refusal triggered fallback")?;
    assert_eq!(error.reason(), Reason::Tls);
    assert_eq!(host.census.connections(), (2, 1));
    assert_eq!(host.census.requests.load(Ordering::SeqCst), 0);
    host.close().await
}
