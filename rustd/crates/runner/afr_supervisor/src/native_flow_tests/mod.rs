//! Native frame observations prove independent control and an idle runner's census.

mod pressure;

use std::time::Duration;

use crate::{
    config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL},
    test_support::{Behaviour, FakeAgent, FakeEngine, healthy},
    test_util::{NativeDaemon, Session},
};
use af_core::{env::MapEnv, id::Uuid7, test_util::trace::Capture};
use af_wire::activity::{ActivityFrame, ActivityRequest, ToolCallProgress};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};

type TestResult<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Frames {
    receiver: mpsc::UnboundedReceiver<serde_json::Value>,
    census: Vec<serde_json::Value>,
}

impl Frames {
    async fn next(&mut self) -> TestResult<serde_json::Value> {
        let frame = tokio::time::timeout(Duration::from_secs(5), self.receiver.recv())
            .await?
            .ok_or("native frame missing")?;
        self.census.push(frame.clone());
        Ok(frame)
    }

    async fn ready(&mut self, minimum: u64) -> TestResult {
        loop {
            let frame = self.next().await?;
            if kind(&frame) == Some("ready")
                && frame
                    .get("credits")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|credits| credits >= minimum)
            {
                return Ok(());
            }
        }
    }

    fn collect(&mut self) {
        while let Ok(frame) = self.receiver.try_recv() {
            self.census.push(frame);
        }
    }

    fn assert_idle(&self) {
        assert!(
            self.census
                .iter()
                .all(|frame| kind(frame).is_some_and(|kind| [
                    "hello",
                    "confirm",
                    "ready",
                    "heartbeat",
                    "ping"
                ]
                .contains(&kind))),
            "an idle runner writes only opening, credit, heartbeat and ping frames: {:?}",
            self.census
        );
        assert_eq!(
            self.census
                .iter()
                .filter(|frame| kind(frame) == Some("hello"))
                .count(),
            1,
            "idle traffic keeps the original native connection"
        );
    }
}

fn kind(frame: &serde_json::Value) -> Option<&str> {
    frame.get("type").and_then(serde_json::Value::as_str)
}

struct Native {
    daemon: NativeDaemon,
    config: Config,
    frames: Frames,
    release_activity: CancellationToken,
    _root: tempfile::TempDir,
}

impl Native {
    async fn idle() -> TestResult<Self> {
        let release_activity = CancellationToken::new();
        let blocked = release_activity.clone();
        let (seen, receiver) = mpsc::unbounded_channel();
        let mut assigned = healthy();
        assigned.interval_ms = 10_000;
        let daemon = NativeDaemon::serve_idle(assigned, move |frame| {
            let seen = seen.clone();
            let release = blocked.clone();
            async move {
                let activity = kind(&frame) == Some("activity");
                let _seen = seen.send(frame);
                if activity {
                    release.cancelled().await;
                }
            }
        })
        .await?;
        let root = tempfile::tempdir()?;
        let issuer = daemon.root();
        let env = MapEnv::from_pairs([
            (ENV_API_URL, "http://127.0.0.1:1"),
            (ENV_STREAM_URL, daemon.url.as_str()),
            (
                ENV_STREAM_CA,
                issuer.to_str().ok_or("fixture root is not UTF-8")?,
            ),
            (ENV_RUNNER_TOKEN, "agt_r_fixture"),
            (
                crate::config::ENV_STORAGE_HOME,
                root.path().to_str().ok_or("fixture path is not UTF-8")?,
            ),
        ]);
        Ok(Self {
            daemon,
            config: Config::from_env(&env)?,
            frames: Frames {
                receiver,
                census: Vec::new(),
            },
            release_activity,
            _root: root,
        })
    }

    fn runner(&self) -> TestResult<(AbortOnDropHandle<crate::Result<()>>, CancellationToken)> {
        let config = self.config.clone();
        let home = crate::StorageHome::open(config.storage_home())?;
        let shutdown = CancellationToken::new();
        let stopped = shutdown.clone();
        let running = AbortOnDropHandle::new(tokio::spawn(async move {
            crate::run(
                &config,
                home,
                Box::new(FakeEngine::default()),
                Box::new(FakeAgent::new(Behaviour::Answer)),
                probe(),
                stopped,
            )
            .await
        }));
        Ok((running, shutdown))
    }

    async fn close(&mut self) -> TestResult {
        self.release_activity.cancel();
        self.daemon.close().await?;
        Ok(())
    }
}

fn probe() -> afr_sandbox::HostProbe {
    afr_sandbox::HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned()],
        bubblewrap: true,
        kvm: afr_sandbox::Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
        egress: false,
    }
}

async fn observe_progress(logs: &Capture, round: usize) -> TestResult {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let events = logs.events();
            let completed = |verb| {
                events
                    .iter()
                    .filter(|event| {
                        event.field("event") == Some("runner_command_completed")
                            && event.field("verb") == Some(verb)
                    })
                    .count()
            };
            if completed("ping") >= round && completed("heartbeat") > round {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    Ok(())
}

async fn observe_idle_window(logs: &Capture, frames: &mut Frames) -> TestResult {
    // Native I/O runs between clock steps, with each ping acknowledged before the next step.
    for round in 1..=4 {
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(15)).await;
        tokio::time::resume();
        observe_progress(logs, round).await?;
        frames.collect();
    }
    Ok(())
}

#[tokio::test]
async fn an_activity_flood_does_not_delay_renewal() -> TestResult {
    let mut native = Native::idle().await?;
    let mut session = Session::connect(&native.config).await?;
    native.frames.ready(0).await?;
    let lease = Uuid7::parse(crate::test_util::LEASE_ID)?;
    let activity = ActivityRequest {
        frames: vec![ActivityFrame::ToolCallProgress(ToolCallProgress {
            name: "shell".into(),
            elapsed_ms: 1,
            call_id: "activity-fixture".into(),
        })],
    };
    session.activity(&lease, &activity).await?;
    assert_eq!(kind(&native.frames.next().await?), Some("activity"));
    for _ in 1..1000 {
        session.activity(&lease, &activity).await?;
    }
    let renewed = session
        .renew(&lease, &af_wire::report::RenewRequest::default())
        .await?;
    assert_eq!(
        kind(&native.frames.next().await?),
        Some("renew_batch"),
        "control renewals reach the wire while the activity stream is blocked"
    );
    assert_eq!(renewed.lease_expires_at, healthy().granted_until);
    native.release_activity.cancel();
    session.close().await?;
    native.close().await
}

#[tokio::test]
async fn an_idle_runner_sends_no_lease_request_over_sixty_seconds() -> TestResult {
    let logs = Capture::install();
    let mut native = Native::idle().await?;
    let (mut running, shutdown) = native.runner()?;
    native.frames.ready(1).await?;
    let started = Instant::now();
    observe_idle_window(&logs, &mut native.frames).await?;
    assert!(started.elapsed() >= Duration::from_secs(60));
    native.frames.assert_idle();
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), &mut running).await???;
    native.close().await
}
