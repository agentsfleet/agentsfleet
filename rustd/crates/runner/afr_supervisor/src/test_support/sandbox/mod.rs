//! A sandbox engine for the run tests: sandboxes that count their teardowns
//! and an executor that reports what is written into the workspace.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use af_core::id::Uuid7;
use af_wire::runner::{Isolation, SandboxEngine};
use afr_engine::{
    Allowlist, Capabilities, Engine, Environment, Granted, Limits, Network, ParkMode,
    ResourceFacts, Root, Sandbox, SandboxSpec, Sweep, Swept,
};
use bytes::Bytes;
use tokio::sync::mpsc;

use crate::egress::Bound;

mod executor;
mod lifecycle;
use executor::FakeExecutor;

/// What the fake engine states: every cap enforced, parking by pause.
pub(crate) const CAPABILITIES: Capabilities = Capabilities {
    engine: SandboxEngine::Firecracker,
    isolation: Isolation::HardwareVm,
    parks: &[ParkMode::Pause],
    snapshots: false,
    resources: ResourceFacts::ENFORCED,
};

/// What a fake sandbox does when it is parked and when it is resumed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Parking {
    /// Parks and resumes.
    #[default]
    Works,
    /// Refuses the park, as a host without the cgroup freezer would; the
    /// engine destroys the sandbox.
    RefusesPark,
    /// Parks, then refuses the resume; the engine destroys the sandbox.
    RefusesResume,
    /// Parks and resumes, but its executor never answers again, as one that
    /// died while parked would.
    ResumesSilent,
    /// Parks and resumes, but its executor fails every listing after.
    ResumesBroken,
    /// Parks, then refuses the addresses its next run's hosts resolved
    /// to, as a kernel refusing the swap would.
    RefusesReallow,
    /// A detached tenant process prevents a live workspace snapshot.
    BusyTenant,
    /// An abandoned file worker has not finished its executor-side work.
    BusyExecutor,
    /// Its executor cannot answer whether file work is idle.
    ExecutorUnproved,
    /// It cannot answer whether a tenant process is still running.
    QuiescenceUnproved,
    /// Its processes ended after the run's calls, so it no longer reports
    /// running once they were served.
    Dead,
    /// The host ended it once it was built, as an overcommitted host's
    /// kernel ends a microVM's monitor: it reports ended before the run's
    /// first call.
    Lost,
    /// Its engine offers no park mode, so a finished run's sandbox is killed.
    Unoffered,
}

/// How the fake engine's every build ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Builds {
    /// It builds.
    #[default]
    Ready,
    /// It refuses, knowing it built nothing.
    Refused,
    /// It cannot tell what it built.
    Ambiguous,
}

/// How a fake sandbox's kill ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Kill {
    /// The tree empties.
    #[default]
    Settles,
    /// It cannot prove the tree empty, as one stuck in the kernel.
    Unsettles,
}

/// An engine whose sandboxes count their teardowns.
#[derive(Debug, Default)]
pub(crate) struct FakeEngine {
    /// How every build ends.
    pub(crate) build: Builds,
    /// Panics on the first prepare only, the way a bug in the supervisor
    /// would take its worker down.
    pub(crate) panic_once: bool,
    /// Cancelled just before that panic, as a shutdown arriving while the
    /// doomed run runs; set once the token to cancel exists.
    pub(crate) halt_before_panic: Arc<std::sync::OnceLock<tokio_util::sync::CancellationToken>>,
    pub(crate) fail_teardown: bool,
    /// How its sandboxes' kills end.
    pub(crate) kill: Kill,
    /// Where each sandbox reports its kill and its destroy, in order, for a
    /// suite that sets the report beside them.
    pub(crate) steps: Option<mpsc::UnboundedSender<&'static str>>,
    /// How long each teardown takes before it is counted.
    pub(crate) teardown_takes: Duration,
    /// How long each prepare takes before it answers, refused or not.
    pub(crate) prepare_takes: Duration,
    /// How long each resume takes before it answers.
    pub(crate) resume_takes: Duration,
    /// How long each quiescence read takes before it answers.
    pub(crate) quiesce_takes: Duration,
    /// How many quiescence reads its sandboxes started.
    pub(crate) quiesced: Arc<AtomicUsize>,
    /// Where each prepare reports when it answered with a sandbox.
    pub(crate) ready: Option<mpsc::UnboundedSender<tokio::time::Instant>>,
    pub(crate) prepared: Arc<AtomicUsize>,
    pub(crate) destroyed: Arc<AtomicUsize>,
    /// Where each sweep reports the scope it was asked for: the sandbox's
    /// identifier, or none for leftovers.
    pub(crate) swept: Option<mpsc::UnboundedSender<Option<Uuid7>>>,
    /// Where each sandbox's executor reports the files written into it.
    pub(crate) written: Option<mpsc::UnboundedSender<(String, Bytes)>>,
    /// The host directory each sandbox's executor imports into and exports
    /// from, as a real executor serves its workspace; none refuses both.
    pub(crate) workspace: Option<PathBuf>,
    /// The verified toolbox digest a durable-recovery sandbox's root is;
    /// none roots it on the host.
    pub(crate) toolbox: Option<String>,
    /// Where each prepare reports the limits it was granted.
    pub(crate) asked: Option<mpsc::UnboundedSender<Limits>>,
    /// Where each prepare reports the identifier it was asked to build.
    pub(crate) named: Option<mpsc::UnboundedSender<Uuid7>>,
    /// Where each prepare reports the network it was asked to reach.
    pub(crate) reached: Option<mpsc::UnboundedSender<Bound>>,
    /// How many times its sandboxes were parked, and resumed.
    pub(crate) parked: Arc<AtomicUsize>,
    pub(crate) resumed: Arc<AtomicUsize>,
    /// Attempts include a resume that reports failure before it can succeed.
    pub(crate) resume_attempts: Arc<AtomicUsize>,
    /// Where each sandbox reports the allowlist it was told to take, and
    /// whether it had been resumed by then.
    pub(crate) reallowed: Option<mpsc::UnboundedSender<(Allowlist, bool)>>,
    /// What its sandboxes do when parked and resumed.
    pub(crate) parking: Parking,
}

impl FakeEngine {
    /// A sandbox built for the fixture run with the default limits, named by
    /// the run as a run's own build names it.
    pub(crate) async fn sandbox(&self) -> afr_engine::Result<Box<dyn Sandbox>> {
        let run = Uuid7::parse(crate::test_util::RUN_ID).unwrap();
        let spec = SandboxSpec::granted_by(self, &run, Limits::default())?;
        self.prepare(spec).await
    }
}

#[async_trait::async_trait]
impl Engine for FakeEngine {
    fn capabilities(&self) -> Capabilities {
        if self.parking == Parking::Unoffered {
            Capabilities {
                parks: &[],
                ..CAPABILITIES
            }
        } else {
            CAPABILITIES
        }
    }

    fn grant(&self, limits: Limits) -> afr_engine::Result<Granted> {
        CAPABILITIES.grant(limits)
    }

    fn memory_bound(&self) -> Option<afr_engine::MemoryBound> {
        None
    }

    async fn sweep(&self, scope: Sweep<'_>) -> afr_engine::Result<Swept> {
        if let Some(swept) = &self.swept {
            let named = match scope {
                Sweep::Leftovers => None,
                Sweep::Sandbox(id) => Some(id.clone()),
            };
            // A test that dropped its receiver is not watching sweeps.
            drop(swept.send(named));
        }
        Ok(Swept::default())
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> afr_engine::Result<Box<dyn Sandbox>> {
        // A test that dropped its receiver is not watching these.
        if let Some(asked) = &self.asked {
            _ = asked.send(*spec.granted.limits());
        }
        if let Some(named) = &self.named {
            drop(named.send(spec.id.clone()));
        }
        if let Some(reached) = &self.reached {
            drop(reached.send(bound(spec.egress.network)));
        }
        if !self.prepare_takes.is_zero() {
            tokio::time::sleep(self.prepare_takes).await;
        }
        match self.build {
            Builds::Ready => {}
            Builds::Refused => return Err(std::io::Error::other("no landlock").into()),
            Builds::Ambiguous => return Err(afr_engine::ambiguous(spec.id, "prepare")),
        }
        let prepared = self.prepared.fetch_add(1, Ordering::SeqCst);
        if self.panic_once
            && prepared == 0
            && let Some(halt) = self.halt_before_panic.get()
        {
            halt.cancel();
        }
        assert!(
            !(self.panic_once && prepared == 0),
            "the fake engine panics on its first prepare"
        );
        if let Some(ready) = &self.ready {
            let _reader_gone = ready.send(tokio::time::Instant::now());
        }
        let root = self
            .toolbox
            .clone()
            .map_or(Root::Host, |digest| Root::Toolbox {
                digest: digest.into_boxed_str(),
            });
        Ok(Box::new(FakeSandbox {
            id: spec.id.clone(),
            granted: spec.granted,
            environment: Environment {
                engine: CAPABILITIES.engine,
                root,
            },
            fail_teardown: self.fail_teardown,
            kill: self.kill,
            steps: self.steps.clone(),
            teardown_takes: self.teardown_takes,
            resume_takes: self.resume_takes,
            quiesce_takes: self.quiesce_takes,
            quiesced: Arc::clone(&self.quiesced),
            destroyed: Arc::clone(&self.destroyed),
            parked: Arc::clone(&self.parked),
            resumed: Arc::clone(&self.resumed),
            resume_attempts: Arc::clone(&self.resume_attempts),
            reallowed: self.reallowed.clone(),
            parking: self.parking,
            executor: FakeExecutor {
                written: self.written.clone(),
                workspace: self.workspace.clone(),
                parking: self.parking,
                resumed: AtomicBool::default(),
                served: AtomicBool::default(),
            },
        }))
    }
}

#[derive(Debug)]
struct FakeSandbox {
    id: Uuid7,
    granted: Granted,
    environment: Environment,
    fail_teardown: bool,
    kill: Kill,
    steps: Option<mpsc::UnboundedSender<&'static str>>,
    teardown_takes: Duration,
    resume_takes: Duration,
    quiesce_takes: Duration,
    quiesced: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
    parked: Arc<AtomicUsize>,
    resumed: Arc<AtomicUsize>,
    resume_attempts: Arc<AtomicUsize>,
    reallowed: Option<mpsc::UnboundedSender<(Allowlist, bool)>>,
    parking: Parking,
    executor: FakeExecutor,
}

impl FakeSandbox {
    /// Reports `step` to the suite reading the steps, if one is.
    fn step(&self, step: &'static str) {
        if let Some(steps) = &self.steps {
            let _reader_gone = steps.send(step);
        }
    }

    /// Removes what it held, counted.
    async fn teardown(&self) -> afr_engine::Result<()> {
        self.step(DESTROY);
        if !self.teardown_takes.is_zero() {
            tokio::time::sleep(self.teardown_takes).await;
        }
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        if self.fail_teardown {
            return Err(std::io::Error::other("busy mount").into());
        }
        Ok(())
    }

    /// Fails `detail` after the engine destroyed the sandbox, as a park or
    /// resume that would not finish does.
    async fn destroyed_on(&self, detail: &'static str) -> afr_engine::Error {
        let _left_for_the_sweep = self.teardown().await;
        std::io::Error::other(detail).into()
    }
}

/// The steps a fake sandbox records, by name.
pub(crate) const KILL: &str = "kill";
pub(crate) const DESTROY: &str = "destroy";
/// What a kill that cannot prove the tree empty says.
pub(crate) const STILL_RUNNING: &str = "a process is stuck in the kernel";

/// What a refused park or resume says.
pub(crate) const NO_PARK: &str = "cgroup.freeze refused";
/// What a sandbox that cannot tell whether a tenant process runs says.
const NO_QUIESCENCE: &str = "cgroup.events unreadable";
/// What a refused swap of a held sandbox's addresses says.
pub(crate) const NO_REFILL: &str = "the egress set refused its new addresses";
/// What a broken executor says once its sandbox is resumed.
pub(crate) const EXECUTOR_GONE: &str = "the executor died while parked";

/// The network a sandbox was asked for, owned, so a test reads it back after
/// the request is gone.
fn bound(network: Network<'_>) -> Bound {
    match network {
        Network::Host => Bound::Host,
        Network::Isolated => Bound::Isolated,
        Network::Allowed(allowlist) => Bound::Allowed(allowlist.clone()),
    }
}
