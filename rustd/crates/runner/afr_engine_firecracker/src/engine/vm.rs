//! A running microVM, and one stopped between two runs of its fleet with
//! only its disk kept.

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use af_core::id::Uuid7;
use afr_engine::error::egress_refused;
use afr_engine::{
    EgressPlan, EgressRefusal, Enforcer, Environment, Generation, Granted, Killed, Network,
    ParkMode, Parked, Quiescence, Result, Sandbox, SandboxState, Snapshots, enforce,
};
use afr_executor::{Client, Ending, Executor, Spawn};

use super::build::{Parts, boot};
use super::{Kept, Plan, Reach, Shared, lost};
use crate::capabilities::CAPABILITIES;

/// What flushes the guest's page cache to its drives before a stop: with the
/// workspace drive on `Writeback`, a synced write survives the monitor's end.
const SYNC: &str = "sync";
/// Why a park whose guest would not sync refuses.
const UNSYNCED: &str = "the guest's sync did not end cleanly";

/// One running microVM and the connection to its executor.
#[derive(Debug)]
pub(crate) struct Vm {
    shared: Arc<Shared>,
    kept: Kept,
    client: Client,
    parts: Parts,
    /// Whether [`Sandbox::kill`] ended its monitor, so its end is not a loss.
    killed: AtomicBool,
}

impl Vm {
    /// The microVM `kept` describes, booted with `parts`, its executor on
    /// `client`.
    pub(crate) const fn new(shared: Arc<Shared>, kept: Kept, client: Client, parts: Parts) -> Self {
        Self {
            shared,
            kept,
            client,
            parts,
            killed: AtomicBool::new(false),
        }
    }

    /// Whether its monitor still runs. One the host ended while the engine
    /// held it running, which no kill of the engine's ended, is logged lost
    /// the first time it is found ended: its run's calls fail, and only it.
    fn monitor(&mut self) -> SandboxState {
        let killed = self.killed.load(Ordering::Acquire);
        let running = self.parts.running(|how| {
            if !killed {
                lost(how);
            }
        });
        if running {
            SandboxState::Running
        } else {
            SandboxState::Ended
        }
    }

    /// Stops the monitor and releases every piece of its boot; the disk and
    /// the directory stay. Hands back what a resume boots from.
    pub(crate) async fn release(mut self: Box<Self>) -> Kept {
        let _found = self.monitor();
        let Self {
            kept,
            client,
            mut parts,
            ..
        } = *self;
        drop(client);
        parts.release(&kept).await;
        kept
    }

    /// Flushes the guest's writes to its drives through its executor.
    async fn sync(&self) -> Result<()> {
        let process = self.client.spawn(&Spawn::program(SYNC)).await?;
        match process.ended(|_stream, _bytes| {}).await {
            Some(Ending::Exited(0)) => Ok(()),
            _unsynced => Err(afr_engine::error::unconfined(UNSYNCED)),
        }
    }
}

#[async_trait::async_trait]
impl Sandbox for Vm {
    fn id(&self) -> &Uuid7 {
        &self.kept.id
    }

    fn environment(&self) -> &Environment {
        &self.kept.environment
    }

    fn granted(&self) -> &Granted {
        &self.kept.granted
    }

    fn executor(&self) -> &dyn Executor {
        &self.client
    }

    fn state(&mut self) -> SandboxState {
        self.monitor()
    }

    /// The executor's own proof that nothing it accepted still runs.
    async fn quiesce(&self) -> Result<Quiescence> {
        Ok(if self.client.recovery_idle().await? {
            Quiescence::Quiet
        } else {
            Quiescence::Busy
        })
    }

    async fn park(self: Box<Self>, mode: ParkMode) -> Result<Box<dyn Parked>> {
        let synced = match CAPABILITIES.require_park(mode) {
            Ok(()) => self.sync().await,
            Err(refused) => Err(refused),
        };
        let shared = Arc::clone(&self.shared);
        let kept = self.release().await;
        match synced {
            Ok(()) => Ok(Box::new(Stopped { shared, kept })),
            Err(error) => {
                remove(&kept);
                Err(error)
            }
        }
    }

    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        let within = self.shared.config.tuning.ready_timeout;
        enforce(self, plan, within).await
    }

    fn snapshots(&self) -> Option<&dyn Snapshots> {
        None
    }

    /// Ends the monitor: nothing of the guest runs after it, so nothing
    /// writes to its disk.
    async fn kill(&self) -> Killed {
        let Some(pid) = self
            .parts
            .child
            .as_ref()
            .and_then(tokio::process::Child::id)
        else {
            return Killed::Settled;
        };
        let pid = rustix::process::Pid::from_raw(i32::try_from(pid).unwrap_or(i32::MAX));
        self.killed.store(true, Ordering::Release);
        match pid.map(|pid| rustix::process::kill_process(pid, rustix::process::Signal::KILL)) {
            Some(Err(refused)) if refused != rustix::io::Errno::SRCH => {
                Killed::Unsettled(std::io::Error::from(refused).into())
            }
            _gone => Killed::Settled,
        }
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let kept = self.release().await;
        fs::remove_dir_all(kept.layout.dir())?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl Enforcer for Vm {
    /// A new allowlist refills the tap's set in one transaction; a plan
    /// reaching what the guest was booted to reach, with no set, changes
    /// nothing. A plan for another kind of network is refused: the tap was
    /// chosen at boot.
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        match (
            &self.kept.plan.reach,
            plan.network,
            self.parts.scope.as_mut(),
        ) {
            (Reach::Allowed(_), Network::Allowed(allowlist), Some(scope)) => {
                scope.reallow(allowlist)?;
            }
            (Reach::Public, Network::Host, _) | (Reach::Isolated, Network::Isolated, _) => {}
            _other => return Err(egress_refused(EgressRefusal::NoScope)),
        }
        self.kept.plan = Plan::from(plan);
        Ok(())
    }

    /// The plan given last when the host table reads back what it needs:
    /// the allowlist's addresses in its set, or a public table's empty one;
    /// with no tap, the plan, which nothing changes.
    async fn in_force(&self) -> Result<Option<Generation>> {
        let plan = &self.kept.plan;
        let Some(scope) = self.parts.scope.as_ref() else {
            return Ok(Some(plan.generation));
        };
        let held = scope.enforced()?;
        let wanted = plan.addresses().unwrap_or_default();
        Ok((held == wanted).then_some(plan.generation))
    }
}

/// A microVM whose monitor stopped, its disk kept.
#[derive(Debug)]
struct Stopped {
    shared: Arc<Shared>,
    kept: Kept,
}

#[async_trait::async_trait]
impl Parked for Stopped {
    fn id(&self) -> &Uuid7 {
        &self.kept.id
    }

    fn mode(&self) -> ParkMode {
        ParkMode::Stop
    }

    /// Boots a fresh guest on the kept disk, held to the plan given last,
    /// and dials its executor again. A host at its memory bound refuses
    /// before any monitor starts, and the disk goes, as any refused resume's
    /// does.
    async fn resume(self: Box<Self>) -> Result<Box<dyn Sandbox>> {
        let Self { shared, kept } = *self;
        let dir = kept.layout.dir().to_owned();
        let booted = match shared.admission.admit() {
            Ok(reservation) => boot(&shared, kept, reservation).await,
            Err(refused) => Err(refused),
        };
        match booted {
            Ok(vm) => Ok(Box::new(vm)),
            Err(error) => {
                let _left_for_the_sweep = fs::remove_dir_all(dir);
                Err(error)
            }
        }
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        fs::remove_dir_all(self.kept.layout.dir())?;
        Ok(())
    }
}

/// Removes a sandbox's directory, image included, leaving what will not go
/// for the boot sweep.
fn remove(kept: &Kept) {
    let _left_for_the_sweep = fs::remove_dir_all(kept.layout.dir());
}
