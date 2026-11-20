//! Everything a standalone harness lends its generic tools, in one place:
//! this host's processes and files under one directory, HTTPS to the public
//! hosts an allowlist names, a memory that lives as long as the process, and
//! the sessions and wake a run keeps.
//!
//! An embedder that runs untrusted work, or holds what a command must not
//! read, builds its [`Context`] over its own sandbox instead.

use std::io;
use std::path::Path;

use crate::tools::outbound::Allowlist;
use crate::tools::{Context, LocalEnv, LocalMemory, LocalOutbound, Sessions, Wake};

/// A standalone run's environment, network, memory, sessions and wake.
#[derive(Debug)]
pub struct Local {
    /// This host's processes and the files under the root.
    pub env: LocalEnv,
    /// HTTPS to the allowed hosts, never to a private address.
    pub outbound: LocalOutbound,
    /// The run's memory.
    pub memory: LocalMemory,
    /// The processes the run keeps open across calls.
    pub sessions: Sessions,
    /// What a new message pokes, so a sleeping call wakes.
    pub wake: Wake,
    /// What names the run on the tools' log lines.
    pub run_id: String,
    /// Whether the model's wire takes an image with a call's result.
    pub image_input: bool,
}

impl Local {
    /// A run named `run_id` over the files under `root`, sending to the hosts
    /// `allowlist` names.
    ///
    /// # Errors
    /// `root` does not exist or cannot be resolved, or the TLS backend
    /// refused the HTTP client's configuration.
    pub fn new(
        run_id: impl Into<String>,
        root: impl AsRef<Path>,
        allowlist: Allowlist,
    ) -> io::Result<Self> {
        Ok(Self {
            env: LocalEnv::new(root)?,
            outbound: LocalOutbound::new(allowlist).map_err(io::Error::other)?,
            memory: LocalMemory::default(),
            sessions: Sessions::default(),
            wake: Wake::default(),
            run_id: run_id.into(),
            image_input: false,
        })
    }

    /// What one call is lent.
    #[must_use]
    pub fn context(&self) -> Context<'_> {
        Context {
            env: Some(&self.env),
            outbound: &self.outbound,
            memory: &self.memory,
            sessions: &self.sessions,
            image_input: self.image_input,
            repository: None,
            run_id: &self.run_id,
            wake: Some(&self.wake),
        }
    }
}
