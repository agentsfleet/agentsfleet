#![expect(clippy::unwrap_used, reason = "test fixtures fail loudly")]

use super::run;
use crate::test_support::{Behaviour, FakeAgent, FakeEngine, bounded};
use afr_sandbox::{HostProbe, Kvm};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn probe() -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned()],
        bubblewrap: true,
        kvm: Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
        egress: false,
    }
}

#[tokio::test]
async fn an_unreachable_daemon_is_retried_until_shutdown() {
    bounded("an_unreachable_daemon_is_retried_until_shutdown", async {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().to_str().unwrap();
        let env = af_core::env::MapEnv::from_pairs([
            (crate::config::ENV_API_URL, "http://127.0.0.1:1"),
            (crate::config::ENV_STREAM_URL, "https://localhost:1"),
            (crate::config::ENV_RUNNER_TOKEN, "agt_r_token"),
            (crate::config::ENV_STORAGE_HOME, home),
        ]);
        let (config, home) = crate::boot(&env).unwrap();
        let shutdown = CancellationToken::new();
        let stopping = async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            shutdown.cancel();
        };

        let (served, ()) = tokio::join!(
            Box::pin(run(
                &config,
                home,
                Box::new(FakeEngine::default()),
                Box::new(FakeAgent::new(Behaviour::Answer)),
                probe(),
                shutdown.clone()
            )),
            stopping
        );

        assert!(
            served.is_ok(),
            "a daemon that never answers is waited out, not fatal"
        );
    })
    .await;
}

#[tokio::test]
async fn shutdown_during_a_blackholed_handshake_finishes_cleanly() {
    bounded(
        "shutdown during a handshake",
        Box::pin(async {
            let blackhole = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let origin = format!(
                "https://127.0.0.1:{}",
                blackhole.local_addr().unwrap().port()
            );
            let root = tempfile::tempdir().unwrap();
            let env = af_core::env::MapEnv::from_pairs([
                (crate::config::ENV_API_URL, "http://127.0.0.1:1"),
                (crate::config::ENV_STREAM_URL, origin.as_str()),
                (crate::config::ENV_RUNNER_TOKEN, "agt_r_fixture"),
                (
                    crate::config::ENV_STORAGE_HOME,
                    root.path().to_str().unwrap(),
                ),
            ]);
            let (config, home) = crate::boot(&env).unwrap();
            let shutdown = CancellationToken::new();
            let cancel = async {
                let mut packet = [0; 2048];
                blackhole.recv(&mut packet).await.unwrap();
                shutdown.cancel();
            };
            let (result, ()) = tokio::join!(
                run(
                    &config,
                    home,
                    Box::new(FakeEngine::default()),
                    Box::new(FakeAgent::new(Behaviour::Answer)),
                    probe(),
                    shutdown.clone()
                ),
                cancel
            );
            assert!(
                result.is_ok(),
                "caller shutdown during a native handshake must succeed: {result:?}"
            );
        }),
    )
    .await;
}
