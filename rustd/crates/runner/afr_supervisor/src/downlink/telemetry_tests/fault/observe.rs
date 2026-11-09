//! Inspect the production runner exporter and host log records, with no replacement filter.

use super::{Failure, Result};
use af_core::env::MapEnv;
use afr_telemetry::{Endpoint, Telemetry};
use axum::{Router, extract::State, routing::post};
use parking_lot::Mutex;
use serde_json::Value;
use std::{io::Write, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Notify};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt as _};

#[derive(Clone, Default)]
struct Logs {
    bytes: Arc<Mutex<Vec<u8>>>,
    changed: Arc<Notify>,
}

impl<'a> MakeWriter<'a> for Logs {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}
impl Write for Logs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes.lock().extend_from_slice(buf);
        self.changed.notify_waiters();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) struct Evidence {
    pub(super) exports: Vec<Value>,
    pub(super) logs: String,
}

pub(super) struct Observer {
    telemetry: Telemetry,
    received: Arc<Mutex<Vec<Value>>>,
    logs: Logs,
    stop: CancellationToken,
    server: AbortOnDropHandle<std::io::Result<()>>,
}

impl Observer {
    pub(super) async fn start() -> Result<Self, Failure> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let received = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/v1/traces", post(accept))
            .route("/v1/metrics", post(|| async { "" }))
            .with_state(Arc::clone(&received));
        let stop = CancellationToken::new();
        let stopping = stop.clone();
        let server = AbortOnDropHandle::new(tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(stopping.cancelled_owned())
                .await
        }));
        let env = MapEnv::from_pairs([
            ("OTEL_EXPORTER_OTLP_ENDPOINT", url.as_str()),
            ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json"),
        ]);
        let endpoint = Endpoint::from_env(&env)?.ok_or("collector endpoint missing")?;
        let telemetry = Telemetry::install(&endpoint)?;
        Ok(Self {
            telemetry,
            received,
            logs: Logs::default(),
            stop,
            server,
        })
    }

    pub(super) fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + 'static {
        tracing_subscriber::registry()
            .with(self.telemetry.layer())
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .without_time()
                    .with_ansi(false)
                    .with_current_span(false)
                    .with_span_list(true)
                    .with_writer(self.logs.clone()),
            )
    }

    pub(super) async fn wait_retries(&self, count: usize) -> Result<(), Failure> {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let changed = self.logs.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let bytes = self.logs.bytes.lock().clone();
                let seen = String::from_utf8_lossy(&bytes)
                    .lines()
                    .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                    .filter(|entry| {
                        entry.pointer("/fields/event").and_then(Value::as_str)
                            == Some("runner_stream_reconnecting")
                    })
                    .count();
                if seen >= count {
                    break;
                }
                changed.await;
            }
        })
        .await?;
        Ok(())
    }

    pub(super) async fn finish(self) -> Result<Evidence, Failure> {
        tokio::task::spawn_blocking(move || self.telemetry.close()).await?;
        self.stop.cancel();
        self.server.await??;
        Ok(Evidence {
            exports: self.received.lock().clone(),
            logs: String::from_utf8(self.logs.bytes.lock().clone())?,
        })
    }
}

async fn accept(
    State(received): State<Arc<Mutex<Vec<Value>>>>,
    body: axum::body::Bytes,
) -> axum::http::StatusCode {
    let Ok(value) = serde_json::from_slice(&body) else {
        return axum::http::StatusCode::BAD_REQUEST;
    };
    received.lock().push(value);
    axum::http::StatusCode::OK
}
