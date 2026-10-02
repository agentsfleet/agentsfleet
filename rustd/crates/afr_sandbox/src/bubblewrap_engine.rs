//! The bubblewrap engine: one hardened sandbox per lease, built from the
//! toolbox, the lease's own cgroup and its own workspace disk.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use afr_executor::{Client, Executor};
use backon::{ConstantBuilder, Retryable as _};

use crate::bubblewrap::{self, Layout, SOCKET_NAME};
use crate::cgroup::{DEFAULT_IO_BYTES_PER_SECOND, LeaseCgroup};
use crate::engine::{Engine, Sandbox, SandboxRequest};
use crate::error::{ErrorKind, Result, refused};
use crate::host::HostTools;
use crate::probe::{ProbePaths, probe};
use crate::toolbox::Toolbox;
use crate::workspace_disk::WorkspaceDisk;

mod parts;

use self::parts::Parts;

/// The directory each lease's executor socket is made in.
const RUN_DIR: &str = "run";
/// How often the socket is tried while the executor binds it.
const CONNECT_DELAY: Duration = Duration::from_millis(2);
/// The event a lease's sandbox start is logged under.
const EVENT_PREPARE_STARTED: &str = "sandbox_prepare_started";
/// The event a ready sandbox is logged under.
const EVENT_PREPARE_COMPLETED: &str = "sandbox_prepare_completed";
/// The event a sandbox that could not be built is logged under.
const EVENT_REFUSED: &str = "sandbox_refused";

/// Everything the engine builds sandboxes from.
#[derive(Debug, Clone)]
pub struct BubblewrapConfig {
    /// The host programs it runs.
    pub tools: HostTools,
    /// Where the host's capabilities are read.
    pub probe: ProbePaths,
    /// The mounted, verified toolbox.
    pub toolbox: Toolbox,
    /// The delegated cgroup each lease's cgroup is made under.
    pub cgroup_root: PathBuf,
    /// Where each lease's directory is made.
    pub state_dir: PathBuf,
    /// The binary that hardens and serves inside; `agentsfleet-runner`.
    pub entry: PathBuf,
    /// What it is told; `sandbox`.
    pub entry_args: Vec<OsString>,
    /// How long a sandbox may take to answer before the lease is refused.
    pub ready_timeout: Duration,
}

/// Builds one bubblewrap sandbox per lease.
#[derive(Debug)]
pub struct BubblewrapEngine {
    config: BubblewrapConfig,
    owner: (u32, u32),
}

impl BubblewrapEngine {
    /// An engine for this host, or a refusal naming what it lacks.
    ///
    /// # Errors
    /// The host lacks Landlock, seccomp, bubblewrap, the toolbox's file system
    /// or a cgroup controller, so no sandbox could be built.
    pub fn new(config: BubblewrapConfig) -> Result<Self> {
        if let Some(missing) = probe(&config.probe).missing() {
            let event = EVENT_REFUSED;
            tracing::error!(missing, event, "this host cannot build a sandbox");
            return Err(refused(missing));
        }
        let owner = (
            rustix::process::getuid().as_raw(),
            rustix::process::getgid().as_raw(),
        );
        Ok(Self { config, owner })
    }

    async fn build(&self, parts: &mut Parts, request: SandboxRequest<'_>) -> Result<Client> {
        let disk = WorkspaceDisk::create(
            &self.config.tools,
            parts.dir(),
            request.limits.disk_bytes,
            self.owner,
        )
        .await?;
        let disk = parts.adopt_disk(disk);
        let workspace = disk.mount_point().to_owned();
        let device = disk.device()?;
        let cgroup = parts.adopt_cgroup(LeaseCgroup::create(
            &self.config.cgroup_root,
            request.lease_id,
            &request.limits,
        )?);
        cgroup.limit_io(device, DEFAULT_IO_BYTES_PER_SECOND)?;
        let procs = cgroup.procs();
        let run_dir = parts.dir().join(RUN_DIR);
        fs::create_dir(&run_dir)?;
        let argv = bubblewrap::arguments(&Layout {
            toolbox: self.config.toolbox.root(),
            workspace: &workspace,
            run_dir: &run_dir,
            entry: &self.config.entry,
            entry_args: &self.config.entry_args,
        });
        parts.spawn(&self.config.tools.bwrap, argv, &procs)?;
        self.ready(parts, &run_dir.join(SOCKET_NAME)).await
    }

    /// Connects once the executor answers, or refuses when the sandbox exits
    /// first or the timeout passes.
    async fn ready(&self, parts: &mut Parts, socket: &Path) -> Result<Client> {
        let waited = self.config.ready_timeout;
        let tries =
            usize::try_from(waited.as_millis() / CONNECT_DELAY.as_millis()).unwrap_or(usize::MAX);
        let backoff = ConstantBuilder::default()
            .with_delay(CONNECT_DELAY)
            .with_max_times(tries);
        let connect = (|| Client::connect(socket)).retry(backoff);
        tokio::select! {
            client = connect => client.map_err(|_unanswered| ErrorKind::NotReady { waited }.into()),
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
        let dir = request.lease_dir(&self.config.state_dir)?;
        fs::create_dir_all(&self.config.state_dir)?;
        // Fresh, never reused: a directory already there belongs to a lease
        // this one must not inherit, and the boot sweep is what removes it.
        fs::create_dir(&dir)?;
        let mut parts = Parts::new(dir);
        match self.build(&mut parts, request).await {
            Ok(client) => {
                let event = EVENT_PREPARE_COMPLETED;
                tracing::info!(lease_id, event);
                Ok(Box::new(Bubblewrapped { client, parts }))
            }
            Err(error) => {
                let reason = error.to_string();
                let error_code = error.code().as_str();
                let event = EVENT_REFUSED;
                tracing::error!(
                    lease_id,
                    error_code,
                    reason,
                    event,
                    "the lease's sandbox could not be built"
                );
                parts.teardown_after_refusal(lease_id).await;
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
}

#[async_trait::async_trait]
impl Sandbox for Bubblewrapped {
    fn executor(&self) -> &dyn Executor {
        &self.client
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let Self { client, parts } = *self;
        drop(client);
        parts.teardown().await
    }
}

#[cfg(test)]
mod tests;
