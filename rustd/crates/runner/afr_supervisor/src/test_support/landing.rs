//! A fresh sandbox a restore lands in: its guest agent serves a workspace
//! directory the test reads on the host, and the restore's spools go beside
//! it.

use std::path::Path;

use af_wire::durable_task::CheckpointSandbox;
use af_wire::runner::SandboxEngine;
use afr_engine::{Environment, Sandbox};

use super::FakeEngine;
use crate::recovery::Landing;

/// A built sandbox on a verified toolbox, its workspace, and where a
/// restore into it packs its tree.
#[derive(Debug)]
pub(crate) struct Lander {
    workspace: tempfile::TempDir,
    spools: tempfile::TempDir,
    sandbox: Box<dyn Sandbox>,
}

impl Lander {
    /// A sandbox on the toolbox `toolbox` names, its workspace empty.
    ///
    /// # Panics
    /// A directory or the sandbox could not be made: a broken fixture.
    pub(crate) async fn on(toolbox: &str) -> Self {
        let (workspace, spools) = (directory(), directory());
        let engine = FakeEngine {
            workspace: Some(workspace.path().to_owned()),
            toolbox: Some(toolbox.to_owned()),
            ..FakeEngine::default()
        };
        let sandbox = engine
            .sandbox()
            .await
            .unwrap_or_else(|failure| panic!("the fake sandbox would not build: {failure}"));
        Self {
            workspace,
            spools,
            sandbox,
        }
    }

    /// Where a restore lands in it.
    pub(crate) fn landing(&self) -> Landing<'_> {
        Landing {
            sandbox: &*self.sandbox,
            spools: self.spools.path(),
        }
    }

    /// What its code runs on: what a checkpoint taken in it records.
    pub(crate) fn environment(&self) -> Environment {
        self.sandbox.environment().clone()
    }

    /// Its workspace, as the host reads it.
    pub(crate) fn workspace(&self) -> &Path {
        self.workspace.path()
    }
}

/// The sandbox a checkpoint taken in a fake engine's sandbox records.
pub(crate) const fn recorded() -> CheckpointSandbox {
    CheckpointSandbox {
        engine: SandboxEngine::Firecracker,
        guest_agent_digest: None,
    }
}

/// A fresh directory, removed when dropped.
fn directory() -> tempfile::TempDir {
    tempfile::tempdir()
        .unwrap_or_else(|failure| panic!("a fixture directory would not make: {failure}"))
}
