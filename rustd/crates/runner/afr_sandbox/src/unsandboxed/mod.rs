//! An engine with no sandbox, for the lanes that run where bubblewrap cannot.
//!
//! It serves the real executor in-process, over a real socket, in a scratch
//! directory, so the supervisor and the daemon are proven end to end on any
//! developer machine. A release build refuses to construct it: nothing a
//! shipped runner executes ever runs outside a sandbox.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use af_core::id::Uuid7;
use af_wire::runner::{Isolation, SandboxEngine};
use afr_executor::{Client, Executor, Sizes};
use tokio::task::JoinHandle;

use crate::engine::{
    Cap, Capabilities, Engine, Environment, Granted, HostWorkspace, Killed, Limits, ParkMode,
    Parked, Quiescence, ResourceFacts, Root, Sandbox, SandboxSpec, SandboxState, Sweep, Swept,
};
use crate::error::{
    EgressRefusal, ErrorKind, Result, egress_refused, sandbox_exists, unconfined, unsupported,
};
use crate::network::Allowlist;

/// The executor's socket in each run's scratch directory: with no sandbox
/// to inherit a pair, the executor listens on a path.
pub(crate) const SOCKET_NAME: &str = "executor.sock";
/// The workspace directory inside each run's scratch directory.
const WORKSPACE_DIR: &str = "workspace";
/// How long the in-process executor may take to bind its socket.
const CONNECT_WITHIN: Duration = Duration::from_secs(1);
/// How long a closed session may take to end its processes.
const SERVER_GRACE: Duration = Duration::from_secs(5);
/// Why an unconfined sandbox cannot say whether a tenant process remains.
const NO_QUIESCENCE: &str = "its processes run under no cgroup to count";
/// What it states: it confines nothing and parks nothing, so every cap is
/// absent and every sandbox is destroyed between runs.
const CAPABILITIES: Capabilities = Capabilities {
    engine: SandboxEngine::Unsandboxed,
    isolation: Isolation::Unconfined,
    parks: &[],
    resources: ResourceFacts {
        memory: Cap::Absent,
        cpu: Cap::Absent,
        pids: Cap::Absent,
        disk: Cap::Absent,
    },
};
/// Why a kill here proves nothing.
const NO_CGROUP_KILL: &str = "its processes run under no cgroup that a kill could prove empty";

/// Builds an unconfined "sandbox" per run under a scratch directory.
#[derive(Debug)]
pub struct UnsandboxedEngine {
    base: PathBuf,
    /// What each run's executor serves with and its client connects with.
    sizes: Sizes,
}

impl UnsandboxedEngine {
    /// What every unsandboxed engine states, before one is built.
    pub const CAPABILITIES: Capabilities = CAPABILITIES;

    /// An engine whose runs live under `base`. Keep `base` short: a Unix
    /// socket's path is capped near a hundred bytes.
    ///
    /// # Errors
    /// Always in a release build.
    pub fn new(base: PathBuf) -> Result<Self> {
        permitted(cfg!(debug_assertions))?;
        Ok(Self {
            base,
            sizes: Sizes::DEFAULT,
        })
    }

    /// The same engine, each run's executor serving with `sizes` and its
    /// client connecting with them.
    #[must_use]
    pub const fn with_sizes(mut self, sizes: Sizes) -> Self {
        self.sizes = sizes;
        self
    }
}

/// Refuses the engine outside a debug build.
pub(crate) fn permitted(debug_build: bool) -> Result<()> {
    if debug_build {
        Ok(())
    } else {
        Err(ErrorKind::UnsandboxedInRelease.into())
    }
}

#[async_trait::async_trait]
impl Engine for UnsandboxedEngine {
    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    /// Every ask, enforced by nothing: the debug engine says so rather than
    /// refusing, since confining nothing is what it is for.
    fn grant(&self, limits: Limits) -> Result<Granted> {
        Ok(Granted::unenforced(limits))
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        fs::create_dir_all(&self.base)?;
        let dir = self.base.join(spec.id.as_str());
        // One directory per identifier: a second build for a live one fails
        // here, before anything else is made.
        fs::create_dir(&dir).map_err(|error| match error.kind() {
            io::ErrorKind::AlreadyExists => sandbox_exists(spec.id),
            _other => error.into(),
        })?;
        let workspace = dir.join(WORKSPACE_DIR);
        fs::create_dir_all(&workspace)?;
        let workspace_root = workspace.clone();
        let socket = dir.join(SOCKET_NAME);
        let sizes = self.sizes;
        let server = tokio::spawn({
            let socket = socket.clone();
            async move {
                afr_executor::bind(&socket)?
                    .with_sizes(sizes)
                    .serve(&workspace)
                    .await
            }
        });
        match connect(&socket, sizes).await {
            Ok(client) => Ok(Box::new(Unconfined {
                id: spec.id.clone(),
                granted: spec.granted,
                environment: Environment {
                    engine: SandboxEngine::Unsandboxed,
                    root: Root::Host,
                },
                dir,
                workspace: workspace_root,
                client,
                server,
            })),
            Err(error) => {
                server.abort();
                fs::remove_dir_all(&dir)?;
                Err(error)
            }
        }
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        let dirs = match scope {
            Sweep::Sandbox(id) => vec![self.base.join(id.as_str())],
            Sweep::Leftovers => match fs::read_dir(&self.base) {
                Ok(entries) => entries
                    .map(|entry| entry.map(|entry| entry.path()))
                    .collect::<io::Result<_>>()?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
                Err(error) => return Err(error.into()),
            },
        };
        let mut swept = Swept::default();
        for dir in dirs {
            match fs::remove_dir_all(&dir) {
                Ok(()) => swept.destroyed += 1,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_left) => swept.left += 1,
            }
        }
        Ok(swept)
    }
}

/// Connects once the executor has bound its socket, holding the link to the
/// `sizes` it serves with.
async fn connect(socket: &Path, sizes: Sizes) -> Result<Client> {
    Ok(Client::connect_within(socket, CONNECT_WITHIN, sizes).await?)
}

/// A scratch directory with an executor serving it.
#[derive(Debug)]
struct Unconfined {
    id: Uuid7,
    granted: Granted,
    environment: Environment,
    dir: PathBuf,
    /// The directory the executor serves as its workspace.
    workspace: PathBuf,
    client: Client,
    server: JoinHandle<afr_executor::Result<()>>,
}

#[async_trait::async_trait]
impl Sandbox for Unconfined {
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

    fn workspace(&self) -> Option<HostWorkspace<'_>> {
        Some(HostWorkspace {
            root: &self.workspace,
            owner: (
                rustix::process::getuid().as_raw(),
                rustix::process::getgid().as_raw(),
            ),
        })
    }

    fn state(&mut self) -> SandboxState {
        if self.server.is_finished() {
            SandboxState::Ended
        } else {
            SandboxState::Running
        }
    }

    /// Refused: its processes run on the host under no cgroup, so nothing
    /// counts them.
    async fn quiesce(&self) -> Result<Quiescence> {
        Err(unconfined(NO_QUIESCENCE))
    }

    /// Refused, and destroyed: nothing can stop its processes together.
    async fn park(self: Box<Self>, _mode: ParkMode) -> Result<Box<dyn Parked>> {
        self.destroy().await?;
        Err(unsupported(SandboxEngine::Unsandboxed, "park"))
    }

    /// Refused: it shares the host's network and confines nothing to an
    /// allowlist.
    async fn reallow(&mut self, _allowlist: &Allowlist) -> Result<()> {
        Err(egress_refused(EgressRefusal::NoScope))
    }

    /// Unsettled: its processes run on the host under no cgroup, so no kill
    /// can prove them gone. They end with the executor's session, at destroy.
    async fn kill(&self) -> Killed {
        Killed::Unsettled(unconfined(NO_CGROUP_KILL))
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let Self {
            dir,
            client,
            mut server,
            ..
        } = *self;
        // Closing the connection is what ends the executor's session, which
        // ends every process it started; a session that outlives the grace
        // period is cut off.
        drop(client);
        let served = tokio::time::timeout(SERVER_GRACE, &mut server)
            .await
            .map_or_else(
                |_late| {
                    server.abort();
                    Ok(())
                },
                |joined| Ok(joined??),
            );
        fs::remove_dir_all(dir)?;
        served
    }
}

#[cfg(feature = "test-util")]
mod drill;
#[cfg(feature = "test-util")]
pub use self::drill::RecoveryDrillEngine;

#[cfg(test)]
mod tests;
