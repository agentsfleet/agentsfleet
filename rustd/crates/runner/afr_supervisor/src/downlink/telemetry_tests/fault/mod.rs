//! A real downlink falls back, loses its carrier, fails another opening, then recovers.

mod assertions;
mod observe;
mod relay;

use crate::{
    Result,
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    downlink::Downlink,
    halt::Halt,
    uplink::StreamHandle,
};
use af_core::env::MapEnv;
use observe::Observer;
use relay::Relay;
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::instrument::WithSubscriber as _;

type Failure = Box<dyn std::error::Error + Send + Sync>;

#[tokio::test]
async fn stream_failures_and_recovery_emit_correlated_telemetry() -> Result<(), Failure> {
    let observer = Observer::start().await?;
    let mut daemon =
        crate::test_util::NativeDaemon::serve_idle(crate::test_support::healthy(), |_| async {})
            .await?;
    let backend = daemon
        .url
        .parse::<http::Uri>()?
        .port_u16()
        .ok_or("native port missing")?;
    let relay = Relay::start(backend).await?;
    let (handle, downlink, _offers) = Downlink::new(&configured(&daemon, &relay)?)?;
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let subscriber = observer.subscriber();
    let run = async {
        tracing::debug_span!(target: "af_stream::telemetry", "unreviewed_payload", payload = "private-frame-fixture").in_scope(|| {});
        let (served, exercised) = tokio::join!(
            downlink.run(&halt),
            exercise(&handle, &relay, &observer, &shutdown)
        );
        served?;
        exercised
    };
    tokio::time::timeout(Duration::from_secs(30), run.with_subscriber(subscriber)).await??;
    relay.close().await?;
    if let Err(error) = daemon.close().await {
        assert!(
            error.is_retryable(),
            "only the deliberate carrier cut may fail the native peer: {error}"
        );
    }
    let evidence = observer.finish().await?;
    assertions::check(&evidence, &daemon.url)?;
    Ok(())
}

async fn exercise(
    handle: &StreamHandle,
    relay: &Relay,
    observer: &Observer,
    shutdown: &CancellationToken,
) -> Result<(), Failure> {
    handle
        .wait_connected(Instant::now() + Duration::from_secs(8))
        .await?;
    handle.compute(crate::client::stream::Ping).await?;
    relay.cut();
    // One retry follows the established carrier loss; another proves an actual failed reopening.
    observer.wait_retries(2).await?;
    assert!(handle.compute(crate::client::stream::Ping).await.is_err());
    relay.restore();
    handle
        .wait_connected(Instant::now() + Duration::from_secs(8))
        .await?;
    handle.compute(crate::client::stream::Ping).await?;
    shutdown.cancel();
    Ok(())
}

fn configured(daemon: &crate::test_util::NativeDaemon, relay: &Relay) -> Result<Config, Failure> {
    let root = daemon.root();
    let env = MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, relay.url.as_str()),
        (
            ENV_STREAM_CA,
            root.to_str().ok_or("root path is not UTF-8")?,
        ),
        (ENV_RUNNER_TOKEN, "agt_r_private_observability_fixture"),
    ]);
    Ok(Config::from_env(&env)?)
}
