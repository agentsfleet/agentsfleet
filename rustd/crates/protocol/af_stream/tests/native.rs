pub(super) use super::support::{Fixture, Result, deadline};
use af_stream::{Connection, Outgoing, Transport};
use af_wire::protocol::StreamKind;
use http::Request;
use std::{sync::atomic::Ordering, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn duplex(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let mut connection = connect(&fixture, &host, transport).await?;
    let mut lanes = Vec::new();
    for kind in StreamKind::ALL {
        let (writer, body) = Outgoing::pipe();
        let request = Request::builder()
            .method("POST")
            .uri(host.endpoint.uri().clone())
            .header(af_wire::protocol::STREAM_KIND_HEADER, kind.as_str())
            .body(body)?;
        let response = connection.open(request, deadline()).await?;
        assert_eq!(response.status(), http::StatusCode::OK);
        lanes.push((writer, response.into_body().reader()));
    }
    assert_eq!(
        host.connections.load(Ordering::SeqCst),
        1,
        "four lanes share one service context"
    );
    assert_eq!(host.requests.load(Ordering::SeqCst), 4);
    for (writer, reader) in &mut lanes {
        ping(writer, reader, 1).await?;
    }
    let (writer, reader) = lanes
        .get_mut(1)
        .ok_or_else(|| std::io::Error::other("missing work kind"))?;
    let payload = vec![b'x'; af_wire::protocol::MESSAGE_BYTES_MAX];
    let mut answer = vec![0; payload.len()];
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(writer.write_all(&payload), reader.read_exact(&mut answer))?;
        Ok::<_, std::io::Error>(())
    })
    .await??;
    assert_eq!(payload, answer);
    connection.close(deadline()).await?;
    host.close().await
}

#[tokio::test]
async fn four_http3_lanes_share_one_verified_duplex_connection() -> Result {
    duplex(Transport::H3).await
}

#[tokio::test]
async fn four_http2_lanes_share_one_verified_duplex_connection() -> Result {
    duplex(Transport::H2).await
}

async fn soak(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let mut connection = connect(&fixture, &host, transport).await?;
    let (mut writer, body) = Outgoing::pipe();
    let request = Request::builder()
        .method("POST")
        .uri(host.endpoint.uri().clone())
        .body(body)?;
    let response = connection.open(request, deadline()).await?;
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut reader = response.into_body().reader();
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(120) {
        ping(&mut writer, &mut reader, 1).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert!(started.elapsed() >= Duration::from_secs(120));
    assert_eq!(host.connections.load(Ordering::SeqCst), 1);
    assert_eq!(host.requests.load(Ordering::SeqCst), 1);
    ping(&mut writer, &mut reader, 1).await?;
    connection.close(deadline()).await?;
    host.close().await
}

#[tokio::test]
#[ignore = "two-minute native soak: make test-integration-rustd"]
async fn http3_exchanges_with_its_upload_open_for_two_minutes() -> Result {
    soak(Transport::H3).await
}

#[tokio::test]
#[ignore = "two-minute native soak: make test-integration-rustd"]
async fn http2_exchanges_with_its_upload_open_for_two_minutes() -> Result {
    soak(Transport::H2).await
}

async fn pressure(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let mut connection = connect(&fixture, &host, transport).await?;
    let mut control = None;
    let mut blocked = tokio::task::JoinSet::new();
    let mut unread = Vec::new();
    for kind in StreamKind::ALL {
        let (mut writer, body) = Outgoing::pipe();
        let request = Request::builder()
            .method("POST")
            .uri(host.endpoint.uri().clone())
            .header(af_wire::protocol::STREAM_KIND_HEADER, kind.as_str())
            .body(body)?;
        let response = connection.open(request, deadline()).await?;
        if kind == StreamKind::Control {
            control = Some((writer, response.into_body().reader()));
        } else {
            unread.push(response);
            blocked.spawn(async move {
                writer
                    .write_all(&vec![b'x'; af_wire::protocol::MESSAGE_BYTES_MAX])
                    .await
            });
        }
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(100), blocked.join_next())
            .await
            .is_err(),
        "unread streams apply backpressure before a whole large payload is accepted"
    );
    let (mut writer, mut reader) =
        control.ok_or_else(|| std::io::Error::other("missing control kind"))?;
    ping(&mut writer, &mut reader, 4).await?;
    connection.close(deadline()).await?;
    while let Some(result) = blocked.join_next().await {
        assert!(
            result?.is_err(),
            "cancelled uploads release blocked writers"
        );
    }
    drop(unread);
    host.close().await
}

#[tokio::test]
async fn http3_control_progresses_while_every_data_lane_is_blocked() -> Result {
    pressure(Transport::H3).await
}

#[tokio::test]
async fn http2_control_progresses_while_every_data_lane_is_blocked() -> Result {
    pressure(Transport::H2).await
}

async fn ping<W, R>(writer: &mut W, reader: &mut R, count: usize) -> Result
where
    W: tokio::io::AsyncWrite + Unpin,
    R: tokio::io::AsyncRead + Unpin,
{
    tokio::time::timeout(Duration::from_secs(2), async {
        for _ in 0..count {
            writer.write_all(b"ping").await?;
            let mut answer = [0; 4];
            reader.read_exact(&mut answer).await?;
            assert_eq!(&answer, b"ping", "answers arrive before upload EOF");
        }
        Ok::<_, std::io::Error>(())
    })
    .await??;
    Ok(())
}

async fn connect(
    fixture: &Fixture,
    host: &super::support::Host,
    transport: Transport,
) -> Result<Connection> {
    Ok(Connection::connect(
        transport,
        &host.endpoint,
        &fixture.trust,
        &host.stop,
        deadline(),
    )
    .await?)
}

/// The negotiated group is read from TLS, rather than inferred from the settings.
#[tokio::test]
async fn both_native_transports_negotiate_hybrid_post_quantum_key_exchange() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    for transport in [Transport::H3, Transport::H2] {
        let connection = connect(&fixture, &host, transport).await?;
        assert_eq!(
            connection.key_exchange(),
            Some(af_stream::tls::KeyExchange::X25519MlKem768)
        );
        connection.close(deadline()).await?;
    }
    host.close().await
}
