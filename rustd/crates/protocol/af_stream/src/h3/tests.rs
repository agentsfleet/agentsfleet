//! The real private listener must reject a classical-only QUIC client.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use http::{Request, Response};
use tokio_quiche::ClientH3Driver;
use tokio_util::sync::CancellationToken;

use crate::{Endpoint, Incoming, Listener, Outgoing, test_util::IdentityFixture};

type TestResult = crate::Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn daemon_acceptor_refuses_an_x25519_only_http3_client() -> TestResult {
    let identity = IdentityFixture::new(vec!["localhost".to_owned()])?;
    let listener = Listener::bind("[::1]:0".parse()?, identity.identity()).await?;
    let address = listener.local_addr()?;
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&requests);
    let stop = CancellationToken::new();
    let serving = tokio::spawn(listener.serve(
        move |_info| {
            let seen = Arc::clone(&seen);
            tower::service_fn(move |_request: Request<Incoming>| {
                seen.fetch_add(1, Ordering::SeqCst);
                async {
                    Ok::<_, Infallible>(Response::new(Outgoing::new(http_body_util::Empty::<
                        bytes::Bytes,
                    >::new())))
                }
            })
        },
        stop.clone(),
        stop.clone(),
    ));
    let mut endpoint = Endpoint::parse(&format!("https://localhost:{}", address.port()))?;
    endpoint.dial = Some(address);
    let client_stop = stop.child_token();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let super::socket::Connected {
        mut initial,
        receive,
        ..
    } = super::socket::connect(&endpoint, &identity.trust(), &client_stop, deadline).await?;
    initial.ssl_mut().set_curves_list("X25519")?;
    let (driver, _controller) = ClientH3Driver::new(super::settings());
    let (_connection, handshake) = initial.handshake_fut(driver);
    let refused = tokio::time::timeout_at(deadline, handshake).await;
    client_stop.cancel();
    receive.await??;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), serving).await???;
    assert!(refused?.is_err());
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    Ok(())
}
