//! The production filter exports reviewed transport operations and rejects neighboring payload spans.

use super::Telemetry;
use af_core::env::MapEnv;
use af_otlp::OTEL_ENDPOINT_KNOB;
use af_stream::Budget;
use af_wire::protocol::StreamKind;
use tracing::instrument::WithSubscriber as _;
use tracing_subscriber::layer::SubscriberExt as _;

#[tokio::test]
async fn actual_frame_operations_reach_the_runner_exporter()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = crate::endpoint::Endpoint::from_env(&MapEnv::from_pairs([(
        OTEL_ENDPOINT_KNOB,
        "http://127.0.0.1:1",
    )]))?
    .ok_or("export endpoint missing")?;
    let telemetry = Telemetry::install(&endpoint)?;
    let subscriber = tracing_subscriber::registry().with(telemetry.layer());
    async {
        tracing::debug_span!(target: "af_stream::telemetry", "unreviewed_payload", payload = "fixture-private").in_scope(|| {});
        tracing::debug!(target: "af_stream::telemetry", event = "fixture_private_event");
        let stop = tokio_util::sync::CancellationToken::new();
        let frame = af_stream::encode(&"fixture-private", &Budget::default(), StreamKind::Reports, &stop, tokio::time::Instant::now() + std::time::Duration::from_secs(1)).await?;
        let (mut writer, reader) = tokio::io::duplex(64);
        drop(reader);
        assert!(af_stream::write(&mut writer, &frame, &stop, tokio::time::Instant::now() + std::time::Duration::from_secs(1)).await.is_err());
        Ok::<_, af_stream::Error>(())
    }.with_subscriber(subscriber).await?;
    telemetry.flush();
    assert_eq!(
        telemetry.exports().spans_lost().count(),
        2,
        "memory reservation and failed write reached the exporter; payload span and event stayed local"
    );
    Ok(())
}
