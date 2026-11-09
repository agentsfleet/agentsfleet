//! A blocked QUIC path stays visible after HTTP/2 succeeds.

mod support;

use std::{
    io::Write,
    sync::{Arc, Mutex},
    time::Duration,
};

use af_observability::test_util::Capture;
use af_stream::{Budget, Connection, Outgoing, Reader, Reason, Transport};
use af_wire::protocol::StreamKind;
use http::Request;
use serde::Deserialize;
use tokio::io::AsyncWriteExt as _;
use tracing::{Instrument as _, instrument::WithSubscriber as _};
use tracing_subscriber::fmt::MakeWriter;

use support::{Fixture, Result, deadline, proxy::Blackhole};

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl<'a> MakeWriter<'a> for Logs {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Write for Logs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .map_err(|poisoned| std::io::Error::other(poisoned.to_string()))?
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Deserialize)]
struct Entry {
    fields: Fields,
    #[serde(default)]
    spans: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Fields {
    event: Option<String>,
    outcome: Option<String>,
    transport: Option<String>,
    key_exchange: Option<String>,
    post_quantum_key_exchange: Option<bool>,
}

#[tokio::test]
async fn blackhole_fallback_cancellation_and_redaction_reach_exported_telemetry() -> Result {
    let captured = Capture::install();
    let logs = Logs::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(logs.clone())
        .finish();
    let secret = format!("private-message-{}", std::process::id());
    let token = format!("Bearer fixture-{}", std::process::id());
    exercise_observations(&secret, &token)
        .with_subscriber(subscriber)
        .await?;
    assert_exports(&captured);
    let raw = String::from_utf8(
        logs.0
            .lock()
            .map_err(|poisoned| std::io::Error::other(poisoned.to_string()))?
            .clone(),
    )?;
    assert!(!raw.contains(&secret));
    assert!(!raw.contains(&token));
    assert_logs(&raw)
}

async fn exercise_observations(secret: &str, token: &str) -> Result {
    let span = tracing::info_span!(
        "runner_session",
        runner_id = "fixture-runner",
        connection_id = "fixture-connection"
    );
    async {
        fallback_and_redaction(secret, token).await?;
        opening_timeout().await?;
        native_health().await?;
        abandon_partial_frame().await
    }
    .instrument(span)
    .await
}

async fn fallback_and_redaction(secret: &str, token: &str) -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let proxy = Blackhole::start(&host).await?;
    let selected =
        Connection::preferred(&proxy.endpoint, &fixture.trust, &host.stop, deadline()).await?;
    assert_eq!(selected.connection.transport(), Transport::H2);
    let mut connection = selected.connection;
    let (mut writer, body) = Outgoing::pipe();
    let request = Request::builder()
        .method("POST")
        .uri(proxy.endpoint.uri().clone())
        .header(http::header::AUTHORIZATION, token)
        .body(body)?;
    let response = connection.open(request, deadline()).await?;
    writer
        .write_all(&u32::try_from(secret.len())?.to_be_bytes())
        .await?;
    writer.write_all(secret.as_bytes()).await?;
    let mut reader = Reader::new(
        response.into_body().reader(),
        Budget::default(),
        StreamKind::Control,
        host.stop.clone(),
        Duration::from_secs(5),
    );
    let frame = reader
        .next()
        .await?
        .ok_or("echo ended before its payload")?;
    let Err(error) = frame.decode::<af_wire::protocol::Up<'_>>() else {
        return Err("malformed JSON decoded".into());
    };
    assert_eq!(error.reason(), Reason::Protocol);
    assert_eq!(
        host.connections.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    assert_eq!(host.requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    connection.close(deadline()).await?;
    proxy.close().await?;
    host.close().await
}

async fn opening_timeout() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve_without_headers().await?;
    let mut connection = Connection::connect(
        Transport::H2,
        &host.endpoint,
        &fixture.trust,
        &host.stop,
        deadline(),
    )
    .await?;
    let (_writer, body) = Outgoing::pipe();
    let request = Request::builder()
        .method("POST")
        .uri(host.endpoint.uri().clone())
        .body(body)?;
    let expires = tokio::time::Instant::now() + Duration::from_millis(100);
    let Err(error) = connection.open(request, expires).await else {
        return Err("silent headers answered".into());
    };
    assert_eq!(error.reason(), Reason::Timeout);
    connection.close(deadline()).await?;
    host.close().await
}

async fn native_health() -> Result {
    let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
    let host = fixture.serve().await?;
    let mut connection = Connection::connect(
        Transport::H3,
        &host.endpoint,
        &fixture.trust,
        &host.stop,
        deadline(),
    )
    .await?;
    let (mut writer, body) = Outgoing::pipe();
    let request = Request::builder()
        .method("POST")
        .uri(host.endpoint.uri().clone())
        .body(body)?;
    let response = connection.open(request, deadline()).await?;
    let mut reader = Reader::new(
        response.into_body().reader(),
        Budget::default(),
        StreamKind::Control,
        host.stop.clone(),
        Duration::from_secs(5),
    );
    writer.write_all(&2_u32.to_be_bytes()).await?;
    writer.write_all(b"{}").await?;
    let frame = reader.next().await?.ok_or("native echo ended early")?;
    assert_eq!(frame.as_bytes(), b"{}");
    connection.close(deadline()).await?;
    host.close().await
}

async fn abandon_partial_frame() -> Result {
    let (mut output, input) = tokio::io::duplex(8);
    output.write_all(&20_u32.to_be_bytes()).await?;
    output.write_all(b"{").await?;
    let mut reader = Reader::new(
        input,
        Budget::default(),
        StreamKind::Control,
        tokio_util::sync::CancellationToken::new(),
        Duration::from_secs(5),
    );
    let future = reader.next();
    tokio::pin!(future);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(future.as_mut().poll(cx).is_pending()))
            .await
    );
    Ok(())
}

fn assert_native_exports(captured: &Capture) {
    assert!(
        captured.histogram_count(
            "agentsfleet_runner_stream_native_rtt_seconds",
            &[("transport", "http3")]
        ) > 0
    );
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_runner_stream_native_rtt_seconds",
            &[("transport", "http2")]
        ),
        0
    );
    assert!(
        captured.sum(
            "agentsfleet_runner_stream_native_packets_total",
            &[("outcome", "sent")]
        ) > 0
    );
}

fn assert_exports(captured: &Capture) {
    assert_native_exports(captured);
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_connect_total",
            &[("transport", "http3"), ("outcome", "timeout")]
        ),
        1
    );
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_connect_total",
            &[("transport", "http2"), ("outcome", "success")]
        ),
        2
    );
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_fallback_total",
            &[("reason", "timeout")]
        ),
        1
    );
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_timeouts_total",
            &[("stage", "open")]
        ),
        1
    );
    assert_eq!(
        captured.sum(
            "agentsfleet_runner_stream_timeouts_total",
            &[("stage", "handshake")]
        ),
        1
    );
    assert_eq!(
        captured.histogram_count(
            "agentsfleet_runner_stream_operation_seconds",
            &[("stage", "read"), ("outcome", "abandoned")]
        ),
        1
    );
}

fn assert_logs(raw: &str) -> Result {
    let entries: Vec<Entry> = raw
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    for transport in ["http3", "http2"] {
        assert!(entries.iter().any(|entry| {
            entry.fields.event.as_deref() == Some("runner_stream_tls_negotiated")
                && entry.fields.transport.as_deref() == Some(transport)
                && entry.fields.key_exchange.as_deref() == Some("x25519_mlkem768")
                && entry.fields.post_quantum_key_exchange == Some(true)
        }));
    }
    let timeout = entries
        .iter()
        .find(|entry| {
            entry.fields.event.as_deref() == Some("runner_stream_operation_failed")
                && entry.fields.outcome.as_deref() == Some("timeout")
        })
        .ok_or("timeout event missing")?;
    assert!(timeout.spans.iter().any(|span| {
        span.get("runner_id").and_then(serde_json::Value::as_str) == Some("fixture-runner")
    }));
    assert!(
        entries
            .iter()
            .any(|entry| entry.fields.event.as_deref() == Some("runner_stream_fallback_attempt"))
    );
    Ok(())
}
