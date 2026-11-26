//! The bubblewrap engine: one hardened sandbox per run, built from the
//! toolbox, the run's own cgroup and its own workspace disk.

use std::ffi::OsString;
use std::fs::{self, DirBuilder};
use std::io;
use std::net::Ipv4Addr;
use std::os::unix::fs::DirBuilderExt as _;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_cgroup::{Freezer, RunCgroup};
use afr_engine::error::{egress_refused, sandbox_exists, unconfined};
use afr_engine::{
    Allowlist, Capabilities, EgressPlan, EgressRefusal, Enforcer, Engine, Environment, Generation,
    Granted, HostProbe, Killed, Limits, Network, ParkMode, Parked, Quiescence, Result, Root,
    Sandbox, SandboxSpec, SandboxState, Sweep, Swept, enforce, until_in_force,
};
use afr_executor::{Client, Executor};
use afr_guest::{Handover, Inherited, TenantFiles};
use afr_image::{Caching, Toolbox, WorkspaceDisk};

use crate::bubblewrap::{self, CAPABILITIES, Layout, NetworkLayout};
use crate::error::not_ready;

mod boot;
mod config;
mod held;
mod names;
mod parts;
mod stderr;
mod sweep;

pub use self::boot::BubblewrapBackend;
pub use self::config::BubblewrapConfig;
use self::names::{Names, join};
use self::parts::Parts;

/// The host user and group bubblewrap runs as when the runner is root.
///
/// `nobody` and `nogroup` on every Debian-family host, so nothing a sandbox
/// writes is owned by an account that can log in or holds a privilege.
pub const SANDBOX_HOST_IDS: (u32, u32) = (65_534, 65_534);

/// A run's directory: others may pass through to the binds beneath it, but
/// not list or write it.
const RUN_DIR_MODE: u32 = 0o711;
/// What the ready probe lists: the workspace, the executor's root.
const READY_PROBE: &str = ".";
/// How long a sandbox whose link closed has to exit, so the reason reported
/// is its own.
const EXITED_WITHIN: Duration = Duration::from_secs(1);
/// The event a run's sandbox start is logged under.
const EVENT_PREPARE_STARTED: &str = "sandbox_prepare_started";
/// The event a ready sandbox is logged under.
const EVENT_PREPARE_COMPLETED: &str = "sandbox_prepare_completed";
/// The event a sandbox that could not be built is logged under; the
/// supervisor, which knows what the run was for, logs the refusal itself.
const EVENT_PREPARE_FAILED: &str = "sandbox_prepare_failed";
/// A run whose workspace disk the host caches a second time. The host-wide
/// answer is the boot probe's `workspace_direct_io` check; this names the run.
const EVENT_DISK_BUFFERED: &str = "sandbox_workspace_buffered";
/// The event a host that can build no sandbox at all is logged under.
const EVENT_HOST_REFUSED: &str = "sandbox_host_refused";
/// Why a sandbox whose cgroup was never made cannot be frozen.
const NO_CGROUP: &str = "it was never moved into a cgroup of its own";

/// Builds one bubblewrap sandbox per run.
#[derive(Debug)]
pub struct BubblewrapEngine {
    config: BubblewrapConfig,
    /// Who owns what the sandbox writes, on the host.
    owner: (u32, u32),
    /// Who bubblewrap is started as; `None` when the runner already is not
    /// root, and starts it as itself.
    run_as: Option<(u32, u32)>,
    /// Whether this host holds sandboxes to allowlists, so its egress tables
    /// and links are this engine's to sweep.
    owns_egress: bool,
}

impl BubblewrapEngine {
    /// An engine for the host `host` describes — the caller's probe of
    /// [`BubblewrapConfig::probe_paths`] — or a refusal naming what it lacks.
    /// Sweeps what a previous run left in its state directory first.
    ///
    /// # Errors
    /// The host lacks Landlock, seccomp, bubblewrap, the toolbox's file system
    /// or a cgroup controller, or another process owns its egress.
    pub fn new(config: BubblewrapConfig, host: &HostProbe) -> Result<Self> {
        Self::with_egress(
            config,
            host,
            || Ok(afr_firewall::own_host()?),
            afr_firewall::sweep,
        )
    }

    /// [`Self::new`], taking the host's egress with `own` and sweeping what
    /// a previous run left of it with `sweep`: the running host's in
    /// production, a stated one in the suites.
    fn with_egress(
        config: BubblewrapConfig,
        host: &HostProbe,
        own: impl FnOnce() -> Result<()>,
        sweep: impl FnOnce() -> afr_firewall::Result<()>,
    ) -> Result<Self> {
        boot::admissible(host)?;
        let root = rustix::process::geteuid().is_root();
        let run_as = root.then_some(config.sandbox_ids);
        let owner = run_as.unwrap_or_else(|| {
            (
                rustix::process::getuid().as_raw(),
                rustix::process::getgid().as_raw(),
            )
        });
        // Before the sweep, which would otherwise remove a second process's
        // live links on this host.
        if host.egress {
            own().inspect_err(|error| {
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_HOST_REFUSED;
                {
                    let message = format_args!("this host's egress is owned elsewhere");
                    tracing::error!(message, error_code, reason, event);
                };
            })?;
        }
        let engine = Self {
            config,
            owner,
            run_as,
            owns_egress: host.egress,
        };
        engine.sweep_leftovers(host.egress.then_some(sweep));
        Ok(engine)
    }

    /// What it builds sandboxes from.
    #[must_use]
    pub const fn config(&self) -> &BubblewrapConfig {
        &self.config
    }

    async fn start(&self, spec: SandboxSpec<'_>) -> Result<Bubblewrapped> {
        let dir = self.config.state_dir.join(spec.id.as_str());
        fs::create_dir_all(&self.config.state_dir)?;
        // Fresh, never reused: a directory already there belongs to a live
        // sandbox or to a leftover only a sweep may remove.
        DirBuilder::new()
            .mode(RUN_DIR_MODE)
            .create(&dir)
            .map_err(|error| match error.kind() {
                io::ErrorKind::AlreadyExists => sandbox_exists(spec.id),
                _other => error.into(),
            })?;
        let mut parts = Parts::new(spec.id.as_str(), dir);
        match self
            .build(
                &mut parts,
                spec.id,
                *spec.granted.limits(),
                spec.egress.network,
            )
            .await
        {
            Ok(client) => Ok(Bubblewrapped {
                held: Held::built(spec.egress),
                ready_timeout: self.config.ready_timeout,
                id: spec.id.clone(),
                granted: spec.granted,
                environment: Environment {
                    engine: SandboxEngine::Bubblewrap,
                    root: Root::Toolbox {
                        digest: self.config.toolbox.digest().into(),
                    },
                },
                client,
                parts,
                _toolbox: Arc::clone(&self.config.toolbox),
            }),
            Err(error) => {
                // Released off the runtime; what it could not remove it logs.
                let _logged = parts.teardown().await;
                Err(error)
            }
        }
    }

    async fn build(
        &self,
        parts: &mut Parts,
        id: &Uuid7,
        limits: Limits,
        network: Network<'_>,
    ) -> Result<Client> {
        let (disk, caching) = WorkspaceDisk::create(
            &self.config.tools,
            parts.dir(),
            limits.disk_bytes,
            self.owner,
        )
        .await?;
        if caching == Caching::Buffered {
            let sandbox_id = id.as_str();
            let event = EVENT_DISK_BUFFERED;
            tracing::debug!(sandbox_id, event);
        }
        let device = parts.adopt_disk(disk).device()?;
        let cgroup = parts.adopt_cgroup(RunCgroup::create(
            &self.config.cgroup_root,
            id.as_str(),
            &limits,
        )?);
        cgroup.limit_io(device, self.config.io_bytes_per_second)?;
        let procs = cgroup.procs();
        let tenant = TenantFiles::open(&cgroup.tenant_procs(), &cgroup.tenant_events())?;
        let (handover, link) = Handover::with_link(tenant)?;
        let names = Names::render(parts.dir(), network)?;
        let argv = self.arguments(parts, handover.descriptors(), limits, names.layout());
        parts.spawn(
            &self.config.tools.bwrap,
            argv,
            &procs,
            handover,
            self.run_as,
        )?;
        let client = self.ready(parts, link).await?;
        if let Network::Allowed(allowlist) = network {
            parts.adopt_egress(join(procs, allowlist.clone()).await?);
        }
        Ok(client)
    }

    /// Bubblewrap's command line for the run `parts` holds.
    fn arguments(
        &self,
        parts: &Parts,
        inherited: Inherited,
        limits: Limits,
        network: NetworkLayout<'_>,
    ) -> Vec<OsString> {
        bubblewrap::arguments(&Layout {
            toolbox: self.config.toolbox.root(),
            workspace: parts.workspace(),
            tmp: parts.tmp(),
            entry: &self.config.entry,
            entry_args: &self.config.entry_args,
            log_level: self.config.log_level.as_deref(),
            inherited,
            sizes: self.config.sizes,
            shared_memory_bytes: limits.shared_memory_bytes(),
            network,
        })
    }

    /// The executor on `link`, once it lists the workspace. Nothing polls: the
    /// listing waits on the link until the entry serves it. A sandbox that
    /// exits first is refused with its own reason, and one silent past the
    /// timeout as not ready.
    async fn ready(&self, parts: &mut Parts, link: UnixStream) -> Result<Client> {
        let waited = self.config.ready_timeout;
        let client = Client::connected(link, self.config.sizes).map_err(not_ready(waited))?;
        let listed = tokio::select! {
            exited = parts.exited() => return Err(exited),
            listed = tokio::time::timeout(waited, client.list_dir(READY_PROBE)) => listed,
        };
        match listed {
            Ok(Ok(_workspace)) => Ok(client),
            // The link closes as the entry exits, a moment before bubblewrap
            // does, whose error stream says why.
            Ok(Err(lost)) => Err(tokio::time::timeout(EXITED_WITHIN, parts.exited())
                .await
                .unwrap_or_else(|_still_running| not_ready(waited)(lost))),
            Err(_elapsed) => Err(not_ready(waited)(
                io::Error::from(io::ErrorKind::TimedOut).into(),
            )),
        }
    }
}

#[async_trait::async_trait]
impl Engine for BubblewrapEngine {
    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        CAPABILITIES.grant(limits)
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        let sandbox_id = spec.id.as_str();
        let event = EVENT_PREPARE_STARTED;
        tracing::info!(sandbox_id, event);
        let started = match self.start(spec).await {
            // Nothing runs before the plan reads back in force.
            Ok(sandbox) => {
                match until_in_force(&sandbox, spec.egress.generation, self.config.ready_timeout)
                    .await
                {
                    Ok(()) => Ok(sandbox),
                    Err(error) => {
                        let _logged = Box::new(sandbox).destroy().await;
                        Err(error)
                    }
                }
            }
            Err(error) => Err(error),
        };
        match started {
            Ok(sandbox) => {
                let event = EVENT_PREPARE_COMPLETED;
                tracing::info!(sandbox_id, event);
                Ok(Box::new(sandbox))
            }
            Err(error) => {
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_PREPARE_FAILED;
                tracing::warn!(sandbox_id, error_code, reason, event);
                Err(error)
            }
        }
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        Ok(match scope {
            Sweep::Leftovers => {
                self.sweep_leftovers(self.owns_egress.then_some(afr_firewall::sweep))
            }
            Sweep::Sandbox(id) => self.sweep_sandbox(id),
        })
    }
}

/// The plan a sandbox's egress was last given, and what its kernel set must
/// hold for that plan to be in force.
#[derive(Debug)]
struct Held {
    generation: Generation,
    /// What it was built to reach, which no later plan may change.
    built: Built,
    /// The addresses the kernel set must hold, sorted; none without a set.
    addresses: Option<Vec<Ipv4Addr>>,
}

/// What a sandbox was built to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Built {
    Host,
    Isolated,
    Allowed,
}

impl Held {
    /// What a sandbox built to `plan` holds.
    fn built(plan: EgressPlan<'_>) -> Self {
        let (built, addresses) = match plan.network {
            Network::Host => (Built::Host, None),
            Network::Isolated => (Built::Isolated, None),
            Network::Allowed(allowlist) => (Built::Allowed, Some(sorted(allowlist))),
        };
        Self {
            generation: plan.generation,
            built,
            addresses,
        }
    }
}

/// `allowlist`'s addresses, sorted, as a read-back answers them.
fn sorted(allowlist: &Allowlist) -> Vec<Ipv4Addr> {
    let mut addresses = allowlist.addresses().to_vec();
    addresses.sort_unstable();
    addresses
}

/// A running sandbox and the connection to its executor.
#[derive(Debug)]
struct Bubblewrapped {
    held: Held,
    /// How long a plan may take to read back in force.
    ready_timeout: Duration,
    id: Uuid7,
    granted: Granted,
    environment: Environment,
    client: Client,
    parts: Parts,
    /// The toolbox it runs on, kept only for its lifetime: retention never
    /// unmounts an image a live sandbox still owns.
    _toolbox: Arc<Toolbox>,
}

#[async_trait::async_trait]
impl Sandbox for Bubblewrapped {
    fn id(&self) -> &Uuid7 {
        &self.id
    }

    fn environment(&self) -> &Environment {
        &self.environment
    }

    fn granted(&self) -> &Granted {
        &self.granted
    }

    fn executor(&self) -> &dyn Executor {
        &self.client
    }

    fn state(&mut self) -> SandboxState {
        if self.parts.is_running() {
            SandboxState::Running
        } else {
            SandboxState::Ended
        }
    }

    async fn quiesce(&self) -> Result<Quiescence> {
        Ok(if self.parts.tenant_empty()? {
            Quiescence::Quiet
        } else {
            Quiescence::Busy
        })
    }

    async fn park(self: Box<Self>, mode: ParkMode) -> Result<Box<dyn Parked>> {
        let frozen = match CAPABILITIES.require_park(mode) {
            Ok(()) => settle(self.parts.freezer(), Freezer::freeze).await,
            Err(refused) => Err(refused),
        };
        match frozen {
            Ok(()) => Ok(Box::new(Paused { sandbox: self })),
            Err(error) => {
                // Released off the runtime; what it could not remove it logs.
                let _logged = self.destroy().await;
                Err(error)
            }
        }
    }

    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        let within = self.ready_timeout;
        enforce(self, plan, within).await
    }

    async fn kill(&self) -> Killed {
        self.parts.kill().await
    }

    fn snapshots(&self) -> Option<&dyn afr_engine::Snapshots> {
        None
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let Self { client, parts, .. } = *self;
        drop(client);
        parts.teardown().await
    }
}

#[async_trait::async_trait]
impl Enforcer for Bubblewrapped {
    /// A new allowlist refills the set in one transaction; a plan reaching
    /// what the sandbox was built to reach, with no set, changes nothing. A
    /// plan for another kind of network is refused: the namespace was chosen
    /// at build.
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        match (self.held.built, plan.network) {
            (Built::Allowed, Network::Allowed(allowlist)) => {
                self.parts.reallow(allowlist).await?;
                self.held.addresses = Some(sorted(allowlist));
            }
            (Built::Host, Network::Host) | (Built::Isolated, Network::Isolated) => {}
            _other => return Err(egress_refused(EgressRefusal::NoScope)),
        }
        self.held.generation = plan.generation;
        Ok(())
    }

    /// The plan given last when the kernel set holds its addresses; with no
    /// set, the namespace the sandbox was built with, which nothing changes.
    async fn in_force(&self) -> Result<Option<Generation>> {
        match &self.held.addresses {
            Some(wanted) => {
                let held = self.parts.enforced()?;
                Ok((held.as_ref() == Some(wanted)).then_some(self.held.generation))
            }
            None => Ok(Some(self.held.generation)),
        }
    }
}

/// A sandbox whose every process the cgroup freezer stopped, memory kept.
#[derive(Debug)]
struct Paused {
    sandbox: Box<Bubblewrapped>,
}

#[async_trait::async_trait]
impl Parked for Paused {
    fn id(&self) -> &Uuid7 {
        &self.sandbox.id
    }

    fn mode(&self) -> ParkMode {
        ParkMode::Pause
    }

    async fn resume(self: Box<Self>) -> Result<Box<dyn Sandbox>> {
        let Self { sandbox } = *self;
        match settle(sandbox.parts.freezer(), Freezer::thaw).await {
            Ok(()) => Ok(sandbox),
            Err(error) => {
                let _logged = sandbox.destroy().await;
                Err(error)
            }
        }
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        self.sandbox.destroy().await
    }
}

/// Runs a freeze or a thaw off the async runtime, since settling polls the
/// kernel until the whole tree has stopped or started.
async fn settle(
    freezer: Option<Freezer>,
    step: fn(&Freezer) -> afr_cgroup::Result<()>,
) -> Result<()> {
    let freezer = freezer.ok_or_else(|| unconfined(NO_CGROUP))?;
    Ok(tokio::task::spawn_blocking(move || step(&freezer)).await??)
}

#[cfg(test)]
mod tests;
