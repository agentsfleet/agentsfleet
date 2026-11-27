//! A running microVM, and one stopped between two runs of its fleet with
//! only its disk kept.

use std::fs;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use af_core::id::Uuid7;
use af_guest_wire::Ending;
use afr_engine::error::egress_refused;
use afr_engine::{
    Allowlist, EgressEnforcer, EgressPlan, EgressRefusal, Environment, Generation, Granted, Killed,
    Network, ParkMode, Parked, Quiescence, Result, Sandbox, SandboxState, Snapshots, enforce,
};
use afr_guest_client::{Client, GuestAgent, Spawn};
use rustix::io::Errno;
use rustix::process::{Pid, PidfdFlags, Signal};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;

use super::build::{Parts, boot};
use super::{Kept, Plan, Reach, Shared, lost};
use crate::capabilities::CAPABILITIES;

/// What flushes the guest's page cache to its drives before a stop: with the
/// workspace drive on `Writeback`, a synced write survives the monitor's end.
const SYNC: &str = "sync";
/// Why a park whose guest would not sync refuses.
const UNSYNCED: &str = "the guest's sync did not end cleanly";

/// One running microVM and the connection to its guest agent.
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
    /// The microVM `kept` describes, booted with `parts`, its guest agent on
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
    fn monitor(&self) -> SandboxState {
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
    pub(crate) async fn release(self: Box<Self>) -> Kept {
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

    /// Flushes the guest's writes to its drives through its guest agent.
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

    fn agent(&self) -> &dyn GuestAgent {
        &self.client
    }

    fn state(&self) -> SandboxState {
        self.monitor()
    }

    /// The guest agent's own proof that nothing it accepted still runs; of a
    /// guest whose monitor has ended, that nothing runs at all, with no
    /// guest agent left to ask.
    async fn quiesce(&self) -> Result<Quiescence> {
        if self.monitor() == SandboxState::Ended {
            return Ok(Quiescence::Quiet);
        }
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

    /// Its snapshots, when its build was handed a chunk store.
    fn snapshots(&self) -> Option<&dyn Snapshots> {
        self.kept
            .snapshots
            .is_some()
            .then_some(self as &dyn Snapshots)
    }

    /// Ends the monitor and waits until it has: nothing of the guest runs
    /// after it, so nothing writes to its disk.
    async fn kill(&self) -> Killed {
        let held = self.parts.monitor_pid().map(i32::try_from);
        let Some(Ok(pid)) = held.map(|pid| pid.map(Pid::from_raw)) else {
            return Killed::Settled;
        };
        self.killed.store(true, Ordering::Release);
        match ended(pid).await {
            Ok(()) => {
                // Reaped here, so the next look finds no monitor to ask.
                let _ended = self.monitor();
                Killed::Settled
            }
            Err(unproved) => Killed::Unsettled(unproved.into()),
        }
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let kept = self.release().await;
        fs::remove_dir_all(kept.layout.dir())?;
        Ok(())
    }
}

/// Kills the process `pid` names, when one still runs, and returns once it
/// has ended. The process is held by a descriptor from the first step, so
/// the signal and the wait are for it and never for a later process given
/// its number.
async fn ended(pid: Option<Pid>) -> io::Result<()> {
    let Some(pid) = pid else {
        return Ok(());
    };
    let held = match rustix::process::pidfd_open(pid, PidfdFlags::NONBLOCK) {
        Ok(held) => held,
        Err(Errno::SRCH) => return Ok(()),
        Err(refused) => return Err(refused.into()),
    };
    match rustix::process::pidfd_send_signal(&held, Signal::KILL) {
        Ok(()) | Err(Errno::SRCH) => {}
        Err(refused) => return Err(refused.into()),
    }
    // A process descriptor reads ready once its process has ended.
    // SAFETY: `held` owns its open descriptor, which moves into the
    // registration and stays open and the same until that is dropped.
    let ending = unsafe { AsyncFd::register_with_interest(held, Interest::READABLE) }?;
    ending.readable().await?.retain_ready();
    Ok(())
}

#[async_trait::async_trait]
impl EgressEnforcer for Vm {
    /// A new allowlist refills the tap's set in one transaction, then
    /// replaces the guest's hosts file with its names; a plan reaching what
    /// the guest was booted to reach, with no set, changes nothing. A plan for
    /// another kind of network is refused: the tap was chosen at boot.
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        match (
            &self.kept.plan.reach,
            plan.network,
            self.parts.scope.as_mut(),
        ) {
            (Reach::Allowed(_), Network::Allowed(allowlist), Some(scope)) => {
                scope.refill(allowlist)?;
                resolve(&self.client, allowlist).await?;
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
    /// and dials its guest agent again. A host at its admission bound refuses
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

/// Has the guest resolve `allowlist`'s names and no others: its hosts file
/// is replaced whole, and every lookup inside reads the new one, before this
/// returns, as the tap's set holds the addresses.
async fn resolve(client: &Client, allowlist: &Allowlist) -> Result<()> {
    Ok(client.hosts(&allowlist.hosts_file()).await?)
}

/// Removes a sandbox's directory, image included, leaving what will not go
/// for the boot sweep.
fn remove(kept: &Kept) {
    let _left_for_the_sweep = fs::remove_dir_all(kept.layout.dir());
}

#[path = "snapshot.rs"]
mod snapshot;

#[cfg(test)]
#[path = "vm_tests.rs"]
mod tests;
