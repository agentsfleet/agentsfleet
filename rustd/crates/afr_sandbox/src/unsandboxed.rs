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
use backon::{ConstantBuilder, Retryable as _};
use tokio::task::JoinHandle;

use crate::bubblewrap::SOCKET_NAME;
use crate::engine::{Engine, Sandbox, SandboxRequest};
use crate::error::{ErrorKind, Result};

/// The workspace directory inside each lease's scratch directory.
const WORKSPACE_DIR: &str = "workspace";
/// How often the socket is tried while the executor binds it.
const CONNECT_DELAY: Duration = Duration::from_millis(2);
/// How many tries before the start is abandoned.
const CONNECT_TRIES: usize = 500;
/// How long a closed session may take to end its processes.
const SERVER_GRACE: Duration = Duration::from_secs(5);

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
        let dir = request.lease_dir(&self.base)?;
        let workspace = dir.join(WORKSPACE_DIR);
        fs::create_dir_all(&workspace)?;
        let socket = dir.join(SOCKET_NAME);
        let server = tokio::spawn({
            let socket = socket.clone();
            async move { afr_executor::serve(&socket, &workspace).await }
        });
        match connect(&socket).await {
            Ok(client) => Ok(Box::new(Unconfined {
                dir,
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
    let backoff = ConstantBuilder::default()
        .with_delay(CONNECT_DELAY)
        .with_max_times(CONNECT_TRIES);
    Ok((|| Client::connect(socket)).retry(backoff).await?)
}

/// A scratch directory with an executor serving it.
#[derive(Debug)]
struct Unconfined {
    dir: PathBuf,
    client: Client,
    server: JoinHandle<afr_executor::Result<()>>,
}

#[async_trait::async_trait]
impl Sandbox for Unconfined {
    fn executor(&self) -> &dyn Executor {
        &self.client
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        let Self {
            dir,
            client,
            mut server,
        } = *self;
        // Closing the connection is what ends the executor's session, which
        // ends every process it started; a session that outlives the grace
        // period is cut off.
        drop(client);
        if tokio::time::timeout(SERVER_GRACE, &mut server)
            .await
            .is_err()
        {
            server.abort();
        }
        fs::remove_dir_all(dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
