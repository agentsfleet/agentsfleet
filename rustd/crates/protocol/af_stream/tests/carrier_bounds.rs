//! Each carrier enforces the frame deadlines and the declared-length bound on its own lanes.
//!
//! `framing.rs` proves the same bounds over an in-memory pipe; these run them through a real
//! HTTP/3 or HTTP/2 lane, so a carrier that buffered, retried or reordered bytes would show here.

use super::native::Result;
use super::support::{Fixture, Host, deadline};
use af_stream::{
    Budget, Connection, Endpoint, IncomingReader, Outgoing, Reader, Reason, Transport, write,
};
use af_wire::protocol::{MESSAGE_BYTES_MAX, STREAM_KIND_HEADER, StreamKind};
use std::time::Duration;
use tokio::{
    io::{AsyncWriteExt as _, DuplexStream},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

/// Short enough to keep the suite quick, long enough that loopback never trips it.
const FRAME_DEADLINE: Duration = Duration::from_millis(300);

/// Every refused frame or connection must end inside this, on either carrier.
const ENDS_WITHIN: Duration = Duration::from_secs(2);

/// One echoing lane over one carrier connection.
struct Lane {
    host: Host,
    connection: Connection,
    writer: DuplexStream,
    reader: IncomingReader,
}

impl Lane {
    async fn open(fixture: &Fixture, transport: Transport, kind: StreamKind) -> Result<Self> {
        let host = fixture.serve().await?;
        let mut connection = Connection::connect(
            transport,
            &host.endpoint,
            &fixture.trust,
            &host.stop,
            deadline(),
        )
        .await?;
        let (writer, body) = Outgoing::pipe();
        let request = http::Request::builder()
            .method("POST")
            .uri(host.endpoint.uri().clone())
            .header(STREAM_KIND_HEADER, kind.as_str())
            .body(body)?;
        let response = connection.open(request, deadline()).await?;
        assert_eq!(response.status(), http::StatusCode::OK, "{transport:?}");
        Ok(Self {
            host,
            connection,
            writer,
            reader: response.into_body().reader(),
        })
    }

    fn framed(&mut self, budget: &Budget, kind: StreamKind) -> Reader<&mut IncomingReader> {
        Reader::new(
            &mut self.reader,
            budget.clone(),
            kind,
            CancellationToken::new(),
            FRAME_DEADLINE,
        )
    }

    async fn close(self) -> Result {
        let Self {
            host,
            connection,
            writer,
            reader,
        } = self;
        drop((writer, reader));
        connection.close(deadline()).await?;
        host.close().await
    }
}

async fn oversized(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let mut lane = Lane::open(&fixture, transport, StreamKind::Reports).await?;
    let budget = Budget::new(1, 1)?;
    lane.writer.write_all(&u32::MAX.to_be_bytes()).await?;
    let Err(error) = lane.framed(&budget, StreamKind::Reports).next().await else {
        return Err(format!("{transport:?} accepted an oversized declared length").into());
    };
    assert_eq!(error.reason(), Reason::Protocol, "{transport:?}");
    assert_eq!(
        budget.available(StreamKind::Reports),
        1,
        "{transport:?} reserved memory for a length it refused"
    );
    lane.close().await
}

#[tokio::test]
async fn an_http3_lane_refuses_an_oversized_declared_length_before_allocating() -> Result {
    oversized(Transport::H3).await
}

#[tokio::test]
async fn an_http2_lane_refuses_an_oversized_declared_length_before_allocating() -> Result {
    oversized(Transport::H2).await
}

async fn partial(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let budget = Budget::default();
    for (kind, bytes) in [
        (StreamKind::Control, vec![0]),
        (
            StreamKind::Reports,
            [&20_u32.to_be_bytes()[..], b"{"].concat(),
        ),
    ] {
        let available = budget.available(kind);
        let mut lane = Lane::open(&fixture, transport, kind).await?;
        lane.writer.write_all(&bytes).await?;
        let started = Instant::now();
        let Err(error) = lane.framed(&budget, kind).next().await else {
            return Err(format!("{transport:?} accepted a partial {kind:?} frame").into());
        };
        assert_eq!(error.reason(), Reason::Timeout, "{transport:?} {kind:?}");
        assert!(started.elapsed() < ENDS_WITHIN, "{transport:?} {kind:?}");
        assert_eq!(
            budget.available(kind),
            available,
            "{transport:?} kept the {kind:?} reservation of a frame that timed out"
        );
        lane.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn an_http3_lane_times_out_a_partial_frame_and_returns_its_memory() -> Result {
    partial(Transport::H3).await
}

#[tokio::test]
async fn an_http2_lane_times_out_a_partial_frame_and_returns_its_memory() -> Result {
    partial(Transport::H2).await
}

async fn blocked(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    // The lane's answers are never read, so the echo stalls and flow control stops the upload.
    let mut lane = Lane::open(&fixture, transport, StreamKind::Work).await?;
    let (budget, stop) = (Budget::default(), CancellationToken::new());
    let value = "x".repeat(MESSAGE_BYTES_MAX - 2);
    let frame = af_stream::encode(&value, &budget, StreamKind::Work, &stop, deadline()).await?;
    let started = Instant::now();
    let Err(error) = write(&mut lane.writer, &frame, &stop, started + FRAME_DEADLINE).await else {
        return Err(format!("{transport:?} wrote a whole frame into an unread lane").into());
    };
    assert_eq!(error.reason(), Reason::Timeout, "{transport:?}");
    assert!(started.elapsed() < ENDS_WITHIN, "{transport:?}");
    lane.close().await
}

#[tokio::test]
async fn an_http3_write_into_an_unread_lane_obeys_its_frame_deadline() -> Result {
    blocked(Transport::H3).await
}

#[tokio::test]
async fn an_http2_write_into_an_unread_lane_obeys_its_frame_deadline() -> Result {
    blocked(Transport::H2).await
}

async fn silent(transport: Transport) -> Result {
    let fixture = Fixture::new(vec!["::1".to_owned()], false)?;
    // A peer that accepts TCP and reads UDP but never answers either handshake.
    let tcp = tokio::net::TcpListener::bind("[::1]:0").await?;
    let port = tcp.local_addr()?.port();
    let _udp = tokio::net::UdpSocket::bind(("::1", port)).await?;
    let holding = tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = tcp.accept().await {
            held.push(socket);
        }
    });
    let endpoint = Endpoint::parse(&format!("https://[::1]:{port}"))?;
    let started = Instant::now();
    let outcome = Connection::connect(
        transport,
        &endpoint,
        &fixture.trust,
        &CancellationToken::new(),
        started + FRAME_DEADLINE,
    )
    .await;
    holding.abort();
    let Err(error) = outcome else {
        return Err(format!("{transport:?} connected to a silent peer").into());
    };
    assert_eq!(error.reason(), Reason::Timeout, "{transport:?}");
    assert!(started.elapsed() < ENDS_WITHIN, "{transport:?}");
    Ok(())
}

#[tokio::test]
async fn an_http3_handshake_with_a_silent_peer_obeys_its_deadline() -> Result {
    silent(Transport::H3).await
}

#[tokio::test]
async fn an_http2_handshake_with_a_silent_peer_obeys_its_deadline() -> Result {
    silent(Transport::H2).await
}
