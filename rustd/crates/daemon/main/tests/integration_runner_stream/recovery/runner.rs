//! The real runner executes one metered subprocess; files synchronize only the external process boundary.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use crate::e2e::Scenario;
use af_core::timing::{ENGINE_STOP_GRACE, LEASE_TTL, STREAM_ANSWER_TIMEOUT};
use af_providers::Usage;
use afr_agent::scripted::{ScriptedEngine, Step};
use std::{path::Path, time::Duration};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

pub(super) const ANSWER: &str = "the interrupted native session recovered";
const SCRIPT: &str = "printf '%s\\n' \"$$\" >> \"$1\"; while [ ! -f \"$2\" ]; do sleep 0.05; done";

pub(super) struct Runner {
    marker: tempfile::TempDir,
    _home: tempfile::TempDir,
    sandboxes: tempfile::TempDir,
    stop: CancellationToken,
    task: AbortOnDropHandle<afr_supervisor::Result<()>>,
}

impl Runner {
    pub(super) async fn start(run: &Scenario, origin: &str) -> Self {
        crate::integration_rust_runner::allow_all_egress(run).await;
        let home = tempfile::tempdir().expect("runner storage home");
        let sandboxes = tempfile::tempdir_in("/tmp").expect("short sandbox base");
        let marker = tempfile::tempdir().expect("external command markers");
        let config = configuration(run, origin, home.path());
        let (config, storage) = afr_supervisor::boot(&config).expect("runner configuration");
        let engine = afr_sandbox::UnsandboxedEngine::new(sandboxes.path().to_owned())
            .expect("test-only sandbox engine");
        let agent = script(marker.path());
        let stop = CancellationToken::new();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            afr_supervisor::run(
                &config,
                storage,
                Box::new(engine),
                Box::new(agent),
                probe(),
                shutdown,
            )
            .await
        });
        Self {
            marker,
            _home: home,
            sandboxes,
            stop,
            task: AbortOnDropHandle::new(task),
        }
    }

    pub(super) async fn started(&self) -> u32 {
        tokio::time::timeout(STREAM_ANSWER_TIMEOUT * 4, async {
            loop {
                if let Ok(text) = std::fs::read_to_string(self.marker.path().join("effect"))
                    && !text.trim().is_empty()
                {
                    return text.trim().parse().expect("one spawned process identifier");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the real tool process starts")
    }

    pub(super) fn release(&self) {
        std::fs::write(self.marker.path().join("release"), [])
            .expect("release the external command");
    }

    pub(super) fn executions(&self) -> usize {
        std::fs::read_to_string(self.marker.path().join("effect"))
            .expect("execution marker")
            .lines()
            .count()
    }

    pub(super) async fn stop(mut self) {
        self.stop.cancel();
        tokio::time::timeout(
            LEASE_TTL + ENGINE_STOP_GRACE * 2 + STREAM_ANSWER_TIMEOUT,
            &mut self.task,
        )
        .await
        .expect("runner shutdown is bounded during a report outage")
        .expect("runner task joins")
        .expect("runner shutdown succeeds");
        assert_eq!(
            std::fs::read_dir(self.sandboxes.path())
                .expect("sandbox base")
                .count(),
            0
        );
    }
}

fn configuration(run: &Scenario, origin: &str, home: &Path) -> af_core::env::MapEnv {
    use afr_supervisor::config::{
        ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME, ENV_STREAM_CA, ENV_STREAM_URL,
    };
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    af_core::env::MapEnv::from_pairs([
        (ENV_API_URL, run.base.as_str()),
        (ENV_RUNNER_TOKEN, run.token.as_str()),
        (ENV_STORAGE_HOME, home.to_string_lossy().as_ref()),
        (ENV_STREAM_URL, origin),
        (ENV_STREAM_CA, root.as_str()),
    ])
}

fn script(marker: &Path) -> ScriptedEngine {
    ScriptedEngine::new([
        Step::Spend(Usage {
            input: 12000,
            cached_input: 0,
            cache_write: 0,
            cache_write_5m: 0,
            cache_write_1h: 0,
            output: 4000,
        }),
        Step::Run {
            tool: "shell",
            spawn: afr_executor::Spawn::program("sh")
                .arg("-c")
                .arg(SCRIPT)
                .arg("native-recovery")
                .arg(marker.join("effect").to_string_lossy().as_ref())
                .arg(marker.join("release").to_string_lossy().as_ref()),
        },
        Step::Say(ANSWER.to_owned()),
    ])
}

fn probe() -> afr_sandbox::HostProbe {
    afr_sandbox::HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: afr_sandbox::REQUIRED_CONTROLLERS
            .map(str::to_owned)
            .to_vec(),
        bubblewrap: true,
        kvm: afr_sandbox::Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
        egress: false,
    }
}
