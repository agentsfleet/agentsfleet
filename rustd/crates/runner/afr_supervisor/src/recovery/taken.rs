//! What a checkpoint records of where its files were taken, and the
//! environment a restore of them must land in.
//!
//! ```text
//!   capture   sandbox's environment ─► toolbox + { engine, guest agent }   recorded
//!   restore   recorded ─► SnapshotRef.environment ─► the engine refuses any other
//! ```

use af_wire::durable_task::CheckpointSandbox;
use af_wire::run::TurnWorkspace;
use afr_engine::{Environment, Root};

/// Where a checkpoint's files were taken: the toolbox, and the sandbox when
/// one ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Taken {
    /// The toolbox digest a capture records.
    pub(crate) toolbox: String,
    /// The sandbox it ran in; none for a task that ran with none.
    pub(crate) sandbox: Option<CheckpointSandbox>,
}

impl Taken {
    /// Files taken on `toolbox` with no sandbox: a task's API-only state.
    pub(crate) const fn without_sandbox(toolbox: String) -> Self {
        Self {
            toolbox,
            sandbox: None,
        }
    }

    /// Files taken in a sandbox on `environment`; none when its root is not
    /// our toolbox, which no checkpoint records.
    pub(crate) fn in_sandbox(environment: &Environment) -> Option<Self> {
        environment.root.toolbox().map(|toolbox| Self {
            toolbox: toolbox.to_owned(),
            sandbox: Some(sandbox(environment)),
        })
    }

    /// The files a turn's checkpoint records on `environment`; none as
    /// [`Taken::in_sandbox`].
    pub(crate) fn turn_workspace(environment: &Environment) -> Option<TurnWorkspace> {
        environment
            .root
            .toolbox()
            .map(|toolbox| TurnWorkspace::Files {
                toolbox_digest: toolbox.to_owned(),
                sandbox: sandbox(environment),
            })
    }
}

/// The sandbox `environment` names, as a checkpoint records it.
fn sandbox(environment: &Environment) -> CheckpointSandbox {
    CheckpointSandbox {
        engine: environment.engine,
        guest_agent_digest: environment.guest_agent.as_deref().map(str::to_owned),
    }
}

/// The environment files taken on `toolbox` in `sandbox` restore in.
pub(crate) fn environment(toolbox: &str, sandbox: &CheckpointSandbox) -> Environment {
    Environment {
        engine: sandbox.engine,
        root: Root::Toolbox {
            digest: toolbox.into(),
        },
        guest_agent: sandbox.guest_agent_digest.as_deref().map(Into::into),
    }
}
