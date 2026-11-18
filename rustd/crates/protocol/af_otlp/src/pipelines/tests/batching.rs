//! Real collector requests prove that deployment batching and clocks reach the SDK.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use opentelemetry::logs::{LogRecord as _, Logger as _, LoggerProvider as _};
use opentelemetry::trace::{Span as _, Tracer as _, TracerProvider as _};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{Builder, Duration, Encoding, OTEL_ENDPOINT_KNOB, OtlpConfig, Registry, service};
use crate::{Exports, PipelineSettings};

struct Collector {
    endpoint: String,
    frames: mpsc::UnboundedReceiver<(String, Value)>,
    server: tokio::task::JoinHandle<()>,
}

impl Collector {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("collector socket");
        let endpoint = format!("http://{}", listener.local_addr().expect("bound collector"));
        let (sent, frames) = mpsc::unbounded_channel();
        let router = Router::new()
            .route("/v1/{signal}", post(accept))
            .with_state(sent);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("collector");
        });
        Self {
            endpoint,
            frames,
            server,
        }
    }

    fn config(&self, settings: PipelineSettings) -> OtlpConfig {
        let checked = garde::Unvalidated::new(settings)
            .validate()
            .expect("bounded batching");
        OtlpConfig::new(&self.endpoint, OTEL_ENDPOINT_KNOB)
            .expect("loopback endpoint")
            .with_encoding(Encoding::HttpJson)
            .with_timeout(Duration::from_secs(2))
            .with_pipeline(&checked)
    }

    async fn receive(&mut self, wanted: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (signal, frame) = self.frames.recv().await.expect("collector stays open");
                if signal == wanted {
                    return frame;
                }
            }
        })
        .await
        .expect("the configured batch or interval exports before the default would")
    }

    async fn close(self, exports: Exports) {
        tokio::task::spawn_blocking(move || exports.shutdown())
            .await
            .expect("export threads stop");
        self.server.abort();
    }
}

async fn accept(
    Path(signal): Path<String>,
    State(sent): State<mpsc::UnboundedSender<(String, Value)>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let _accepted = sent.send((signal, body));
    Json(serde_json::json!({}))
}

#[tokio::test]
async fn a_one_span_batch_exports_before_its_long_flush_interval() {
    let mut collector = Collector::start().await;
    let config = collector.config(PipelineSettings {
        trace_batch_size: 1,
        trace_export_interval_seconds: 3600,
        ..Default::default()
    });
    let (exports, _) = Builder::new(&config, service(), Registry::declared().expect("census"))
        .install()
        .expect("real exporters");
    exports
        .tracer()
        .tracer("configured-batch")
        .start("configured-span")
        .end();
    let frame = collector.receive("traces").await;
    assert_eq!(
        frame
            .pointer("/resourceSpans/0/scopeSpans/0/spans/0/name")
            .and_then(Value::as_str),
        Some("configured-span")
    );
    collector.close(exports).await;
}

#[tokio::test]
async fn a_short_span_interval_exports_a_partial_batch() {
    let mut collector = Collector::start().await;
    let config = collector.config(PipelineSettings {
        trace_export_interval_seconds: 1,
        ..Default::default()
    });
    let (exports, _) = Builder::new(&config, service(), Registry::declared().expect("census"))
        .install()
        .expect("real exporters");
    exports
        .tracer()
        .tracer("configured-clock")
        .start("partial-span")
        .end();
    let frame = collector.receive("traces").await;
    assert_eq!(
        frame
            .pointer("/resourceSpans/0/scopeSpans/0/spans/0/name")
            .and_then(Value::as_str),
        Some("partial-span")
    );
    collector.close(exports).await;
}

#[tokio::test]
async fn a_one_record_batch_exports_before_its_long_log_interval() {
    let mut collector = Collector::start().await;
    let config = collector.config(PipelineSettings {
        log_batch_size: 1,
        log_export_interval_seconds: 3600,
        ..Default::default()
    });
    let (exports, _) = Builder::new(&config, service(), Registry::declared().expect("census"))
        .with_logs()
        .install()
        .expect("real exporters");
    let logger = exports
        .logger()
        .expect("configured logs")
        .logger("configured-batch");
    let mut record = logger.create_log_record();
    record.set_body("configured-record".into());
    logger.emit(record);
    let frame = collector.receive("logs").await;
    assert_eq!(
        frame
            .pointer("/resourceLogs/0/scopeLogs/0/logRecords/0/body/stringValue")
            .and_then(Value::as_str),
        Some("configured-record")
    );
    collector.close(exports).await;
}

#[tokio::test]
async fn the_metric_interval_exports_a_recorded_counter_without_a_forced_flush() {
    use af_observability::metrics::declared::cost::BILLING_CREDIT_CONSUMED;
    let mut collector = Collector::start().await;
    let config = collector.config(PipelineSettings {
        metrics_export_interval_seconds: 1,
        ..Default::default()
    });
    let (exports, instruments) =
        Builder::new(&config, service(), Registry::declared().expect("census"))
            .install()
            .expect("real exporters");
    instruments
        .counter_u64(&BILLING_CREDIT_CONSUMED)
        .expect("credit counter")
        .add(17, &[]);
    let frame = collector.receive("metrics").await;
    assert_eq!(
        frame
            .pointer("/resourceMetrics/0/scopeMetrics/0/metrics/0/sum/dataPoints/0/asInt")
            .and_then(Value::as_str),
        Some("17")
    );
    collector.close(exports).await;
}
