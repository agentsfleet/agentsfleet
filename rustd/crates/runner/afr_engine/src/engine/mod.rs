//! The interface every engine meets.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use af_core::id::Uuid7;
use afr_guest_client::GuestAgent;

use crate::error::{Error, Result};
use crate::network::{Network, NetworkKind};

mod bound;
mod egress;
mod facts;
mod snapshots;
pub(crate) mod volume;
pub use self::bound::{AdmissionBound, Overcommitted, Resource};
pub use self::egress::{EgressEnforcer, EgressPlan, Generation, enforce, until_in_force};
pub use self::facts::{Cap, Capabilities, Environment, Granted, ParkMode, ResourceFacts, Root};
pub use self::snapshots::{SnapshotRef, Snapshots};

/// Memory a run's sandbox may use before its tree is killed.
pub const DEFAULT_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Processor share, in thousandths of one core, by default.
///
/// Two cores is Indy's number (Indy, Nov 27, 2026: "CPU per VM 2 cores"). The runner's
/// `[sandbox] cpu_millicores` chooses it.
pub const DEFAULT_CPU_MILLIS: u32 = 2_000;
/// Processes and threads the sandbox's tenant tree may run at once, by
/// default.
///
/// 512, the value Indy ruled enforced (Indy, Nov 27, 2026: "Enforce
/// + TOML (Recommended)"). The runner's `[sandbox] pids_max` chooses it.
pub const DEFAULT_PIDS: u32 = 512;
/// Size of the run's workspace disk; a write past it answers `ENOSPC`.
pub const DEFAULT_DISK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// How long a new sandbox may take to answer before its run is refused, by
/// default: long enough that it ends only a sandbox that will never answer.
pub const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(10);
/// The share of a sandbox's memory its `/dev/shm` may fill: one part in this
/// many. Shared memory is a `tmpfs`, so its pages stay charged to the tenant
/// after the process that wrote them dies; uncapped, one full `/dev/shm`
/// leaves every later command in the run, and in a parked sandbox the next
/// run too, killed for memory at its first allocation.
const SHARED_MEMORY_SHARE: u64 = 4;

/// What one run's sandbox enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Memory, in bytes.
    pub memory_bytes: u64,
    /// Processor share, in thousandths of one core.
    pub cpu_millis: u32,
    /// Processes and threads.
    pub pids: u32,
    /// The workspace disk's size, in bytes.
    pub disk_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Limits {
    /// Every default above, which the runner's file may change.
    pub const DEFAULT: Self = Self {
        memory_bytes: DEFAULT_MEMORY_BYTES,
        cpu_millis: DEFAULT_CPU_MILLIS,
        pids: DEFAULT_PIDS,
        disk_bytes: DEFAULT_DISK_BYTES,
    };

    /// What the sandbox's `/dev/shm` may hold, in bytes: a full one answers
    /// `ENOSPC` while the tenant keeps the rest of its memory.
    #[must_use]
    pub const fn shared_memory_bytes(&self) -> u64 {
        self.memory_bytes.div_euclid(SHARED_MEMORY_SHARE)
    }
}

/// What a sandbox is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxSpec<'a> {
    /// Its identifier, minted by the caller: every directory, cgroup and
    /// device the engine makes for it carries this name, so a leftover of a
    /// build that ended badly is found by it.
    pub id: &'a Uuid7,
    /// What it enforces, as its engine admitted.
    pub granted: Granted,
    /// What its network reaches, numbered: the build returns only once its
    /// enforcer reads this plan back in force.
    pub egress: EgressPlan<'a>,
}

impl<'a> SandboxSpec<'a> {
    /// A sandbox named `id` enforcing `granted`, reaching nothing beyond
    /// loopback until [`SandboxSpec::with_egress`] says otherwise: an engine
    /// never opens a network no one asked for.
    #[must_use]
    pub const fn new(id: &'a Uuid7, granted: Granted) -> Self {
        Self {
            id,
            granted,
            egress: EgressPlan {
                generation: Generation(0),
                network: Network::Isolated,
            },
        }
    }

    /// The spec `engine` builds for `id`: `limits` as `engine` grants them,
    /// and no network yet.
    ///
    /// # Errors
    /// As [`Engine::grant`].
    pub fn granted_by(engine: &dyn Engine, id: &'a Uuid7, limits: Limits) -> Result<Self> {
        Ok(Self::new(id, engine.grant(limits)?))
    }

    /// The same spec, held to `egress`.
    #[must_use]
    pub const fn with_egress(self, egress: EgressPlan<'a>) -> Self {
        Self { egress, ..self }
    }
}

/// A fresh sandbox identifier, minted the one way identifiers are, for a
/// test that builds a sandbox.
///
/// # Panics
/// When the operating system refuses entropy, which fails the test.
#[cfg(any(test, feature = "test-util"))]
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "test support: a host with no entropy fails the test loudly"
)]
pub fn fresh_sandbox_id() -> Uuid7 {
    af_crypto::entropy::Entropy::new()
        .uuid7(af_core::clock::now())
        .expect("the operating system gives entropy")
}

/// Whether a sandbox can still serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxState {
    /// Its guest agent is up.
    Running,
    /// Its guest agent or its processes are gone; it serves nothing more and is
    /// only destroyed.
    Ended,
}

/// What [`Sandbox::quiesce`] read from the engine's own mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiescence {
    /// No tenant process remains; only the resident guest agent may run, and
    /// its memory is never captured.
    Quiet,
    /// A tenant process still runs, so the disk may still change.
    Busy,
}

/// What [`Engine::sweep`] looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sweep<'a> {
    /// Everything this engine made that a previous runner left: at boot,
    /// when no sandbox is live.
    Leftovers,
    /// What a build of unknown outcome may have left of one sandbox.
    Sandbox(&'a Uuid7),
}

/// What [`Engine::sweep`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Swept {
    /// Pieces destroyed: directories, cgroups, devices, processes.
    pub destroyed: usize,
    /// Pieces it found and could not remove.
    pub left: usize,
}

/// What a [`Sandbox::kill`] proves about the processes inside.
#[derive(Debug)]
pub enum Killed {
    /// The process tree is empty: nothing in the sandbox runs, so nothing
    /// writes to its disk.
    Settled,
    /// The kill could not prove the tree empty, and why. A process may still
    /// run and write: the run still reports, and nothing may save the disk
    /// on this record's strength.
    Unsettled(Error),
}

/// Builds one sandbox per run.
#[async_trait::async_trait]
pub trait Engine: Send + Sync + fmt::Debug {
    /// What it is and guarantees every sandbox it builds.
    fn capabilities(&self) -> Capabilities;

    /// `limits` as this engine will enforce them, decided before anything is
    /// built: what a run reports is the grant, never the ask.
    ///
    /// # Errors
    /// A cap this engine has no mechanism for.
    fn grant(&self, limits: Limits) -> Result<Granted>;

    /// The most sandboxes it builds at once under the host's memory and disk,
    /// when it reserves both for each; `None` when only the worker count
    /// bounds it. A build past the bound is refused, and no sandbox starts.
    fn admission_bound(&self) -> Option<AdmissionBound>;

    /// The kind of network the daemon assigned this runner, told on each
    /// assignment that takes work: what an engine building sandboxes ahead
    /// of their runs builds them for. One that builds per run ignores it.
    fn network_assigned(&self, _kind: NetworkKind) {}

    /// Builds the sandbox `spec` names, with its guest agent ready, or refuses.
    ///
    /// A sandbox that cannot be built refuses the run; nothing ever runs
    /// unsandboxed in its place. A build whose outcome the engine cannot
    /// tell answers [`crate::Error::is_ambiguous`]; the caller then sweeps
    /// it once and never builds that identifier again.
    ///
    /// # Errors
    /// The build failed, the identifier names a live sandbox, or the outcome
    /// is unknown.
    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>>;

    /// Destroys what `scope` names that this engine made and no live sandbox
    /// owns.
    ///
    /// # Errors
    /// Nothing could be read; what it found and could not remove is counted
    /// in [`Swept::left`] instead.
    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept>;
}

/// A shared engine is that engine, so one owner can keep a handle the
/// supervisor's box does not consume, as a pool's shutdown needs.
#[async_trait::async_trait]
impl<E: Engine + ?Sized> Engine for Arc<E> {
    fn capabilities(&self) -> Capabilities {
        (**self).capabilities()
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        (**self).grant(limits)
    }

    fn admission_bound(&self) -> Option<AdmissionBound> {
        (**self).admission_bound()
    }

    fn network_assigned(&self, kind: NetworkKind) {
        (**self).network_assigned(kind);
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        (**self).prepare(spec).await
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        (**self).sweep(scope).await
    }
}

/// One run's sandbox, owned by the task running that run.
#[async_trait::async_trait]
pub trait Sandbox: Send + Sync + fmt::Debug {
    /// The identifier it was built under.
    fn id(&self) -> &Uuid7;

    /// What its code runs on, which a checkpoint must match to restore.
    fn environment(&self) -> &Environment;

    /// What it enforces.
    fn granted(&self) -> &Granted;

    /// The guest agent running inside it.
    fn agent(&self) -> &dyn GuestAgent;

    /// Whether it can still serve. A warm slot whose sandbox ended while it
    /// waited is discarded on claim rather than handed to a run, and a run
    /// whose sandbox ended under it ends unavailable. Read through `&self`, so
    /// a run reads it while its turn borrows the guest agent.
    fn state(&self) -> SandboxState;

    /// Whether any tenant process remains, read from the engine's own
    /// mechanism rather than remembered.
    ///
    /// # Errors
    /// The engine cannot tell; a caller treats that as
    /// [`Quiescence::Busy`].
    async fn quiesce(&self) -> Result<Quiescence>;

    /// Parks it in `mode` between two runs of one fleet; returns once it is
    /// parked. A parked sandbox runs nothing and answers nothing until
    /// [`Parked::resume`], so it is a different type.
    ///
    /// # Errors
    /// `mode` is not one its engine offers, or parking could not be
    /// confirmed. The engine has then destroyed it.
    async fn park(self: Box<Self>, mode: ParkMode) -> Result<Box<dyn Parked>>;

    /// Holds the sandbox to `plan` from now on, its names rendered to match,
    /// and returns once its enforcer reads `plan` back in force: what a
    /// resumed sandbox takes before its next run's first command. A plan
    /// older than the one in force is refused.
    ///
    /// # Errors
    /// The plan is stale, asks for a network this sandbox was not built to
    /// reach, the mechanism refused it, or it never read back. A sandbox that
    /// refuses is destroyed.
    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()>;

    /// Its snapshots, when its engine offers them
    /// ([`Capabilities::snapshots`]); none otherwise.
    fn snapshots(&self) -> Option<&dyn Snapshots>;

    /// Ends every process inside and waits, bounded, for the tree to empty.
    /// The disk, the cgroup and the egress rules stay for
    /// [`Sandbox::destroy`], so the run can report before it releases them.
    async fn kill(&self) -> Killed;

    /// Ends every process inside, then removes what it held.
    ///
    /// Takes the sandbox by value: a destroyed sandbox cannot be used again,
    /// and the type system, not a flag, is what says so.
    async fn destroy(self: Box<Self>) -> Result<()>;
}

/// A sandbox parked between two runs of its fleet: it can only resume or go.
#[async_trait::async_trait]
pub trait Parked: Send + Sync + fmt::Debug {
    /// The identifier it was built under.
    fn id(&self) -> &Uuid7;

    /// How it is parked.
    fn mode(&self) -> ParkMode;

    /// Serves again; returns once its guest agent answers.
    ///
    /// # Errors
    /// It would not resume, and the engine has destroyed it; its run gets a
    /// fresh sandbox.
    async fn resume(self: Box<Self>) -> Result<Box<dyn Sandbox>>;

    /// Removes what it held.
    async fn destroy(self: Box<Self>) -> Result<()>;
}

#[cfg(test)]
mod tests;
