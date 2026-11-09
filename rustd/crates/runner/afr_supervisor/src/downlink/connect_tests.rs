#![expect(clippy::unwrap_used, reason = "native test fixtures fail loudly")]

use super::{Opening, Running};
use crate::{
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    downlink::reconnect::Resume,
};
use af_stream::{Budget, Connection, Transport};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn ownership_opening_cannot_spend_the_ping_interval_on_either_carrier() {
    for transport in [Transport::H3, Transport::H2] {
        let mut daemon = stalling_daemon().await;
        let root = daemon.root();
        let config = config(&daemon.url, &root);
        let stop = CancellationToken::new();
        let mut connection = Connection::connect(
            transport,
            &config.stream().endpoint,
            &config.stream().trust,
            &stop,
            Instant::now() + Duration::from_secs(5),
        )
        .await
        .unwrap();
        let (input, output) = Budget::duplex(af_stream::MemoryLimits::default());
        let running = Arc::new(Running::default());
        let authorization = http::header::HeaderValue::from_static("Bearer agt_r_fixture");
        let opening = Opening {
            config: config.stream(),
            authorization: &authorization,
            running: &running,
            input: &input,
            output: &output,
            stop: &stop,
            deadline: Instant::now() + Duration::from_secs(7),
        };
        let mut resume = Resume::default();
        let started = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(4),
            opening.attach(&mut connection, &mut resume),
        )
        .await;
        let error = outcome.unwrap().err().unwrap();
        assert!(matches!(
            error.kind(),
            crate::error::ErrorKind::StreamTimeout { .. }
        ));
        assert_eq!(error.code(), af_core::error_code::RUN_PROTOCOL_UNSUPPORTED);
        assert!(started.elapsed() >= af_core::timing::STREAM_OPEN_TIMEOUT);
        daemon.close().await.unwrap();
        connection
            .close(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
    }
}

async fn stalling_daemon() -> crate::test_util::NativeDaemon {
    crate::test_util::NativeDaemon::serve(
        crate::test_support::healthy(),
        serde_json::Value::Null,
        |frame| async move {
            if frame.get("type").and_then(serde_json::Value::as_str) == Some("hello") {
                std::future::pending::<()>().await;
            }
        },
    )
    .await
    .unwrap()
}

fn config(url: &str, root: &std::path::Path) -> Config {
    let env = af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, "http://127.0.0.1:1"),
        (ENV_STREAM_URL, url),
        (ENV_STREAM_CA, root.to_str().unwrap()),
        (ENV_RUNNER_TOKEN, "agt_r_fixture"),
    ]);
    Config::from_env(&env).unwrap()
}
