//! The bubblewrap engine: one hardened sandbox per lease, built from the
//! toolbox, the lease's own cgroup and its own workspace disk.

use std::ffi::OsString;
use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use afd_core::error_code::{Coded as _, Logged};
use afr_executor::{Client, Executor};
use rustix::fs::{Gid, Uid};

use crate::bubblewrap::{self, Layout, NetworkLayout, SOCKET_NAME};
use crate::cgroup::{DEFAULT_IO_BYTES_PER_SECOND, Freezer, LeaseCgroup};
use crate::engine::{Engine, HostWorkspace, LeaseName, Limits, Sandbox, SandboxRequest};
use crate::error::{Result, not_ready, refused, unconfined};
use crate::network::Network;
use crate::probe::HostProbe;
use crate::tenant::{TenantDescriptors, TenantFiles};
use crate::toolbox::Toolbox;
use crate::workspace_disk::{Caching, WorkspaceDisk};

mod config;
mod names;
mod parts;
mod sweep;

pub use self::config::BubblewrapConfig;
use self::names::{Names, join};
use self::parts::Parts;

/// The host user and group bubblewrap runs as when the runner is root.
///
/// `nobody` and `nogroup` on every Debian-family host, so nothing a sandbox
/// writes is owned by an account that can log in or holds a privilege.
pub const SANDBOX_HOST_IDS: (u32, u32) = (65_534, 65_534);
/// How long a sandbox may take to answer before its lease is refused: the
/// kernel lane measures executor-ready in tens of milliseconds, so this only
/// ends a sandbox that will never answer.
pub const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// The directory each lease's executor socket is made in.
const RUN_DIR: &str = "run";
/// A lease's directory: others may pass through to the binds beneath it, but
/// not list or write it.
const LEASE_DIR_MODE: u32 = 0o711;
/// The socket directory: its owner alone, and the owner is the sandbox.
const RUN_DIR_MODE: u32 = 0o700;
/// The event a lease's sandbox start is logged under.
const EVENT_PREPARE_STARTED: &str = "sandbox_prepare_started";
/// The event a ready sandbox is logged under.
const EVENT_PREPARE_COMPLETED: &str = "sandbox_prepare_completed";
/// The event a sandbox that could not be built is logged under; the
/// supervisor, which knows what the lease was for, logs the refusal itself.
const EVENT_PREPARE_FAILED: &str = "sandbox_prepare_failed";
/// A lease whose workspace disk the host caches a second time. The host-wide
/// answer is the boot probe's `workspace_direct_io` check; this names the lease.
const EVENT_DISK_BUFFERED: &str = "sandbox_workspace_buffered";
/// The event a host that can build no sandbox at all is logged under.
const EVENT_HOST_REFUSED: &str = "sandbox_host_refused";
/// Why a sandbox whose cgroup was never made cannot be frozen.
const NO_CGROUP: &str = "it was never moved into a cgroup of its own";

/// Builds one bubblewrap sandbox per lease.
#[derive(Debug)]
pub struct BubblewrapEngine {
    config: BubblewrapConfig,
    /// Who owns what the sandbox writes, on the host.
    owner: (u32, u32),
    /// Who bubblewrap is started as; `None` when the runner already is not
    /// root, and starts it as itself.
    run_as: Option<(u32, u32)>,
}

impl BubblewrapEngine {
    /// An engine for the host `host` describes — the caller's probe of
    /// [`BubblewrapConfig::probe_paths`] — or a refusal naming what it lacks.
    /// Sweeps what a previous run left in its state directory first.
    ///
    /// # Errors
    /// The host lacks Landlock, seccomp, bubblewrap, the toolbox's file system
    /// or a cgroup controller.
    pub fn new(config: BubblewrapConfig, host: &HostProbe) -> Result<Self> {
        if let Some(missing) = host.missing() {
            let error = refused(missing);
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_HOST_REFUSED;
            tracing::error!(
                missing,
                error_code,
                reason,
                event,
                "this host cannot build a sandbox"
            );
            return Err(error);
        }
        let root = rustix::process::geteuid().is_root();
        let run_as = root.then_some(config.sandbox_ids);
        let owner = run_as.unwrap_or((
            rustix::process::getuid().as_raw(),
            rustix::process::getgid().as_raw(),
        ));
        // Before the sweep, which would otherwise remove a second process's
        // live links on this host.
        if host.egress {
            crate::egress::own_host().inspect_err(|error| {
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_HOST_REFUSED;
                tracing::error!(
                    error_code,
                    reason,
                    event,
                    "this host's egress is owned elsewhere"
                );
            })?;
        }
        let engine = Self {
            config,
            owner,
            run_as,
        };
        engine.sweep(host.egress);
        Ok(engine)
    }

    async fn start(&self, request: SandboxRequest<'_>) -> Result<Bubblewrapped> {
        let name = request.name()?;
        let dir = name.dir_in(&self.config.state_dir);
        fs::create_dir_all(&self.config.state_dir)?;
        // Fresh, never reused: a directory already there belongs to a lease
        // this one must not inherit, and the boot sweep is what removes it.
        DirBuilder::new().mode(LEASE_DIR_MODE).create(&dir)?;
        let mut parts = Parts::new(name.as_str(), dir);
        match self
            .build(&mut parts, name, request.limits, request.network)
            .await
        {
            Ok(client) => Ok(Bubblewrapped {
                client,
                parts,
                owner: self.owner,
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
        name: LeaseName<'_>,
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
            let lease_id = name.as_str();
            let event = EVENT_DISK_BUFFERED;
            tracing::debug!(lease_id, event);
        }
        let device = parts.adopt_disk(disk).device()?;
        let cgroup = parts.adopt_cgroup(LeaseCgroup::create(
            &self.config.cgroup_root,
            name.as_str(),
            &limits,
        )?);
        cgroup.limit_io(device, DEFAULT_IO_BYTES_PER_SECOND)?;
        let procs = cgroup.procs();
        let tenant = TenantFiles::open(&cgroup.tenant_procs(), &cgroup.tenant_events())?;
        let run_dir = self.run_dir(parts.dir())?;
        let names = Names::render(parts.dir(), network)?;
        let argv = self.arguments(
            parts,
            &run_dir,
            tenant.descriptors(),
            limits,
            names.layout(),
        );
        parts.spawn(&self.config.tools.bwrap, argv, &procs, tenant, self.run_as)?;
        let client = self.ready(parts, &run_dir.join(SOCKET_NAME)).await?;
        if let Network::Allowed(allowlist) = network {
            parts.adopt_egress(join(procs, allowlist.clone()).await?);
        }
        Ok(client)
    }

    /// Bubblewrap's command line for the lease `parts` holds.
    fn arguments(
        &self,
        parts: &Parts,
        run_dir: &Path,
        tenant: TenantDescriptors,
        limits: Limits,
        network: NetworkLayout<'_>,
    ) -> Vec<OsString> {
        bubblewrap::arguments(&Layout {
            toolbox: self.config.toolbox.root(),
            workspace: parts.workspace(),
            tmp: parts.tmp(),
            run_dir,
            entry: &self.config.entry,
            entry_args: &self.config.entry_args,
            log_level: self.config.log_level.as_deref(),
            tenant,
            shared_memory_bytes: limits.shared_memory_bytes(),
            network,
        })
    }

    /// The socket directory, owned by the user the sandbox runs as and no one
    /// else: the one place it binds its socket before confining itself.
    fn run_dir(&self, lease_dir: &Path) -> Result<PathBuf> {
        let run_dir = lease_dir.join(RUN_DIR);
        DirBuilder::new().mode(RUN_DIR_MODE).create(&run_dir)?;
        if self.run_as.is_some() {
            let (uid, gid) = (Uid::from_raw(self.owner.0), Gid::from_raw(self.owner.1));
            rustix::fs::chown(&run_dir, Some(uid), Some(gid))?;
        }
        Ok(run_dir)
    }

    /// Connects once the executor answers, or refuses when the sandbox exits
    /// first or the timeout passes.
    async fn ready(&self, parts: &mut Parts, socket: &Path) -> Result<Client> {
        let waited = self.config.ready_timeout;
        tokio::select! {
            client = Client::connect_within(socket, waited) => client.map_err(not_ready(waited)),
            exited = parts.exited() => Err(exited),
        }
    }
}

#[async_trait::async_trait]
impl Engine for BubblewrapEngine {
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        let lease_id = request.lease_id;
        let event = EVENT_PREPARE_STARTED;
        tracing::info!(lease_id, event);
        match self.start(request).await {
            Ok(sandbox) => {
                let event = EVENT_PREPARE_COMPLETED;
                tracing::info!(lease_id, event);
                Ok(Box::new(sandbox))
            }
            Err(error) => {
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_PREPARE_FAILED;
                tracing::warn!(lease_id, error_code, reason, event);
                Err(error)
            }
        }
    }
}

/// A running sandbox and the connection to its executor.
#[derive(Debug)]
struct Bubblewrapped {
    client: Client,
    parts: Parts,
    /// Who owns the workspace disk's files, as the host names them.
    owner: (u32, u32),
    /// The toolbox it runs on, held until it is destroyed so retention never
    /// unmounts it from under a lease or a warm slot.
    _toolbox: Arc<Toolbox>,
}

#[async_trait::async_trait]
impl Sandbox for Bubblewrapped {
    fn executor(&self) -> &dyn Executor {
        &self.client
    }

    fn workspace(&self) -> Option<HostWorkspace<'_>> {
        Some(HostWorkspace {
            root: self.parts.workspace(),
            owner: self.owner,
        })
    }

    fn is_running(&mut self) -> bool {
        self.parts.is_running()
    }

    async fn freeze(&self) -> Result<()> {
        settle(self.parts.freezer(), Freezer::freeze).await
    }

    async fn thaw(&self) -> Result<()> {
        settle(self.parts.freezer(), Freezer::thaw).await
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let Self { client, parts, .. } = *self;
        drop(client);
        parts.teardown().await
    }
}

/// Runs a freeze or a thaw off the async runtime, since settling polls the
/// kernel until the whole tree has stopped or started.
async fn settle(freezer: Option<Freezer>, step: fn(&Freezer) -> Result<()>) -> Result<()> {
    let freezer = freezer.ok_or_else(|| unconfined(NO_CGROUP))?;
    tokio::task::spawn_blocking(move || step(&freezer)).await?
}

#[cfg(test)]
mod tests;
