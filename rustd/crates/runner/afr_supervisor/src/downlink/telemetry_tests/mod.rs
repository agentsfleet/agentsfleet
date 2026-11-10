//! Exercise the production downlink, including its spawned native codec tasks.

use super::Downlink;
use crate::{
    Result,
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    halt::Halt,
};
use af_core::{env::MapEnv, id::Uuid7, test_util::trace::Capture};
use tokio_util::sync::CancellationToken;

mod fault;

#[tokio::test]
async fn a_native_session_records_its_attempt_owner_and_codec_parent()
-> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let capture = Capture::install();
    let mut daemon = crate::test_util::NativeDaemon::serve(
        crate::test_support::healthy(),
        serde_json::Value::Null,
        |_| async {},
    )
    .await?;
    let root = daemon.root();
    let env = MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, daemon.url.as_str()),
        (
            ENV_STREAM_CA,
            root.to_str().ok_or("root path is not UTF-8")?,
        ),
        (ENV_RUNNER_TOKEN, "agt_r_fixture_tracing"),
    ]);
    let config = Config::from_env(&env)?;
    let (handle, downlink, _offers) = Downlink::new(&config)?;
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let exercise = exercise(&handle, &shutdown);
    let (served, exercised) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(downlink.run(&halt), exercise)
    })
    .await?;
    served?;
    exercised?;
    daemon.close().await?;
    assert_session(&capture, &daemon.url)?;
    Ok(())
}

async fn exercise(
    handle: &crate::uplink::StreamHandle,
    shutdown: &CancellationToken,
) -> crate::Result<()> {
    handle
        .wait_connected(tokio::time::Instant::now() + std::time::Duration::from_secs(5))
        .await?;
    handle.compute(crate::client::stream::Ping).await?;
    shutdown.cancel();
    Ok(())
}

fn assert_session(
    capture: &Capture,
    url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let spans = capture.spans();
    let session = spans
        .iter()
        .find(|span| span.name == "runner.connection")
        .ok_or("connection span missing")?;
    Uuid7::parse(
        session
            .field("attempt_id")
            .ok_or("attempt identifier missing")?,
    )?;
    Uuid7::parse(
        session
            .field("connection_id")
            .ok_or("confirmed owner missing")?,
    )?;
    assert!(
        spans
            .iter()
            .any(|span| span.name == "runner_stream_operation"
                && span.parent == Some("runner.connection")
                && span.field("operation") == Some("write"))
    );
    let observed = format!("{:?}{:?}", spans, capture.events());
    assert!(!observed.contains("agt_r_fixture_tracing"));
    assert!(!observed.contains(url));
    Ok(())
}
