//! An engine with no sandbox, for the lanes that run where bubblewrap cannot.
//!
//! It serves the real executor in-process, over a real socket, in a scratch
//! directory, so the supervisor and the daemon are proven end to end on any
//! developer machine. A release build refuses to construct it: nothing a
//! shipped runner executes ever runs outside a sandbox.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use afr_executor::{Client, Executor};
use tokio::task::JoinHandle;

use crate::bubblewrap::SOCKET_NAME;
use crate::engine::{Engine, HostWorkspace, Sandbox, SandboxRequest};
use crate::error::{ErrorKind, Result, unconfined};

/// The workspace directory inside each lease's scratch directory.
const WORKSPACE_DIR: &str = "workspace";
/// How long the in-process executor may take to bind its socket.
const CONNECT_WITHIN: Duration = Duration::from_secs(1);
/// How long a closed session may take to end its processes.
const SERVER_GRACE: Duration = Duration::from_secs(5);
/// Why an unconfined sandbox is never frozen.
const NO_FREEZER: &str = "its processes run under no cgroup to freeze";

/// Builds an unconfined "sandbox" per lease under a scratch directory.
#[derive(Debug)]
pub struct UnsandboxedEngine {
    base: PathBuf,
}

impl UnsandboxedEngine {
    /// An engine whose leases live under `base`. Keep `base` short: a Unix
    /// socket's path is capped near a hundred bytes.
    ///
    /// # Errors
    /// Always in a release build.
    pub fn new(base: PathBuf) -> Result<Self> {
        permitted(cfg!(debug_assertions))?;
        Ok(Self { base })
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
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        let dir = request.name()?.dir_in(&self.base);
        let workspace = dir.join(WORKSPACE_DIR);
        fs::create_dir_all(&workspace)?;
        let workspace_root = workspace.clone();
        let socket = dir.join(SOCKET_NAME);
        let server = tokio::spawn({
            let socket = socket.clone();
            async move { afr_executor::serve(&socket, &workspace).await }
        });
        match connect(&socket).await {
            Ok(client) => Ok(Box::new(Unconfined {
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
}

/// Connects once the executor has bound its socket.
async fn connect(socket: &Path) -> Result<Client> {
    Ok(Client::connect_within(socket, CONNECT_WITHIN).await?)
}

/// A scratch directory with an executor serving it.
#[derive(Debug)]
struct Unconfined {
    dir: PathBuf,
    /// The directory the executor serves as its workspace.
    workspace: PathBuf,
    client: Client,
    server: JoinHandle<afr_executor::Result<()>>,
}

#[async_trait::async_trait]
impl Sandbox for Unconfined {
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

    fn is_running(&mut self) -> bool {
        !self.server.is_finished()
    }

    /// Refused: its processes run on the host under no cgroup, so nothing can
    /// stop them together. It is destroyed, never held.
    async fn freeze(&self) -> Result<()> {
        Err(unconfined(NO_FREEZER))
    }

    async fn thaw(&self) -> Result<()> {
        Err(unconfined(NO_FREEZER))
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

#[cfg(test)]
mod tests;
