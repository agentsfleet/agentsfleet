use af_stream::{Connection, Reason, Transport};

use super::native::{Fixture, Result, deadline};

async fn refused(names: Vec<String>, expired: bool, trust_other: bool) -> Result {
    let fixture = Fixture::new(names, expired)?;
    let other = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let trust = if trust_other {
        &other.trust
    } else {
        &fixture.trust
    };
    for transport in [Transport::H3, Transport::H2] {
        let outcome =
            Connection::connect(transport, &host.endpoint, trust, &host.stop, deadline()).await;
        let Err(error) = outcome else {
            return Err(std::io::Error::other("unverified certificate accepted").into());
        };
        assert_eq!(
            error.reason(),
            Reason::Tls,
            "peer identity failures cannot trigger fallback"
        );
    }
    assert_eq!(host.requests.load(std::sync::atomic::Ordering::SeqCst), 0);
    host.close().await
}

#[tokio::test]
async fn both_transports_refuse_a_certificate_for_another_host() -> Result {
    refused(vec!["other.invalid".to_owned()], false, false).await
}

#[tokio::test]
async fn both_transports_refuse_an_untrusted_certificate() -> Result {
    refused(vec!["localhost".to_owned()], false, true).await
}

#[tokio::test]
async fn both_transports_refuse_an_expired_certificate() -> Result {
    refused(vec!["localhost".to_owned()], true, false).await
}

#[tokio::test]
async fn peer_identity_failure_is_terminal_for_the_preferred_connector() -> Result {
    let fixture = Fixture::new(vec!["wrong.invalid".to_owned()], false)?;
    let host = fixture.serve().await?;
    let outcome =
        Connection::preferred(&host.endpoint, &fixture.trust, &host.stop, deadline()).await;
    let Err(error) = outcome else {
        return Err(std::io::Error::other("identity refusal downgraded").into());
    };
    assert_eq!(error.reason(), Reason::Tls);
    assert_eq!(
        host.connections.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "no HTTP/2 connection followed the identity refusal"
    );
    host.close().await
}

#[tokio::test]
async fn expired_opening_cancels_every_native_worker_on_both_transports() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve_without_headers().await?;
    for transport in [Transport::H3, Transport::H2] {
        let mut connection = Connection::connect(
            transport,
            &host.endpoint,
            &fixture.trust,
            &host.stop,
            deadline(),
        )
        .await?;
        let (_writer, body) = af_stream::Outgoing::pipe();
        let request = http::Request::builder()
            .method("POST")
            .uri(host.endpoint.uri().clone())
            .body(body)?;
        let expires = tokio::time::Instant::now() + std::time::Duration::from_millis(100);
        let outcome = connection.open(request, expires).await;
        let Err(error) = outcome else {
            return Err(std::io::Error::other("silent headers answered").into());
        };
        assert_eq!(error.reason(), Reason::Timeout);
        connection.close(deadline()).await?;
    }
    host.close().await
}

#[tokio::test]
async fn blackholed_quic_falls_back_to_verified_http2_within_one_connection_budget() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let proxy = super::support::proxy::Blackhole::start(&host).await?;
    let started = tokio::time::Instant::now();
    let selected =
        Connection::preferred(&proxy.endpoint, &fixture.trust, &host.stop, deadline()).await?;
    assert_eq!(selected.connection.transport(), Transport::H2);
    assert_eq!(selected.http3_failure, Some(Reason::Timeout));
    assert!(started.elapsed() < std::time::Duration::from_millis(4500));
    selected.connection.close(deadline()).await?;
    host.close().await?;
    proxy.close().await?;
    Ok(())
}
