//! Coded revocation ends the production runner downlink and its recovery owner.

use super::{PUSH_BOUND, TestResult, fixture::Fixture, signals};
use crate::wire;
use af_core::error_code::RUN_ADMIN_STATE_BLOCKED;

// Value: protects=a coded revoke ends the production downlink without an automatic reconnect;
// fails_when=the runner treats the refusal as a transient carrier loss;
// why_new=raw native readers do not execute runner halt policy;
// seam=real runner downlink and authenticated operator HTTP over two daemons.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_operator_revocation_halts_the_production_native_downlink() -> TestResult {
    let fixture = Fixture::start().await?;
    let mut runner = production_runner(&fixture).await?;
    let bytes = serde_json::to_vec(&wire::capable_beat())?;
    let beat = serde_json::from_slice(&bytes)?;
    runner.heartbeat(&beat).await?;
    fixture.operator.runner(&fixture.run, "revoke").await?;
    let stopped = tokio::time::timeout(PUSH_BOUND, runner.offer()).await?;
    assert!(stopped.is_err(), "revoked runners cannot await more offers");
    let failure = runner
        .close()
        .await
        .err()
        .ok_or("revocation did not halt the downlink")?;
    assert_eq!(failure.refusal_code(), Some(RUN_ADMIN_STATE_BLOCKED));
    signals::opening_refused(
        &fixture.run,
        &fixture.run.token,
        RUN_ADMIN_STATE_BLOCKED,
        af_stream::Transport::H2,
    )
    .await?;
    fixture.finish().await
}

async fn production_runner(fixture: &Fixture) -> TestResult<afr_supervisor::test_util::Session> {
    use afr_supervisor::config::{
        Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL,
    };
    let origin = format!(
        "https://localhost:{}",
        fixture.peer.runner_stream_address.port()
    );
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    let config = Config::from_env(&af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, fixture.run.base.as_str()),
        (ENV_RUNNER_TOKEN, fixture.run.token.as_str()),
        (ENV_STREAM_CA, root.as_str()),
        (ENV_STREAM_URL, origin.as_str()),
    ]))?;
    Ok(afr_supervisor::test_util::Session::connect(&config).await?)
}
