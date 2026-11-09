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
