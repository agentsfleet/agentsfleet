//! What `run` leases with on this host: the sandbox engine, the capability
//! report heartbeats carry, and the agent loop every lease's turn runs on.
//!
//! On Linux the engine is bubblewrap, built from the cgroup the service
//! manager delegated, the toolbox release a deploy staged, and the runner
//! binary itself as every sandbox's entry
//! (`docs/architecture/runner_execution.md` §Toolbox). A debug build can be
//! told to build no sandbox at all, so a suite drives this binary end to end on
//! a host without bubblewrap; a release build has no way to ask.

use std::process::ExitCode;
use std::sync::Arc;

use afd_core::error_code::{self, Coded, ErrorCode};
use afr_sandbox::{Engine, HostProbe};
use afr_supervisor::StorageHome;

/// The event every failure that stops `run` is logged under.
pub(crate) const EVENT_RUN_FAILED: &str = "run_failed";
/// Why `run` stopped when the host lacks a mechanism every sandbox needs.
#[cfg(not(target_os = "linux"))]
const CANNOT_SANDBOX: &str = "this host cannot build a sandbox";
/// The status `run` exits with when the daemon refused the runner's token
/// (`EX_NOPERM`). The unit names it in `RestartPreventExitStatus`: restarting
/// does not make a cordoned, drained or revoked token valid again, so the
/// runner stays stopped until an operator restarts it.
const EXIT_TOKEN_REFUSED: u8 = 77;
/// The status every other failure exits with.
const EXIT_FAILED: u8 = 1;
/// The unit's `DelegateSubgroup`: only its parent belongs to this service.
#[cfg(target_os = "linux")]
const RUNNER_SUBGROUP: &str = "runner";

/// The engine and the facts `run` serves leases with.
pub(crate) struct Host {
    /// Builds each lease's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// What this host's kernel can enforce, as every heartbeat states it.
    pub(crate) probe: HostProbe,
    /// Backend resources, released after serving and its runtime have ended.
    pub(crate) resources: Option<Box<dyn afr_sandbox::Resources>>,
}

impl Host {
    /// A host leasing with `engine`, on a kernel `probe` describes, its
    /// sandboxes using `resources`. A release build off Linux builds no
    /// engine at all, so it has no caller there.
    #[cfg(any(target_os = "linux", debug_assertions))]
    fn new(
        engine: impl Engine + 'static,
        probe: HostProbe,
        resources: Option<Box<dyn afr_sandbox::Resources>>,
    ) -> Self {
        Self {
            engine: Box::new(engine),
            probe,
            resources,
        }
    }

    /// The bubblewrap engine this host builds sandboxes with, booted over the
    /// release a deploy staged (`afr_sandbox::boot` with `BubblewrapBackend`).
    ///
    /// # Errors
    /// The reason it cannot, already logged, as the exit status.
    #[cfg(target_os = "linux")]
    pub(crate) fn production(home: &StorageHome) -> Result<Self, ExitCode> {
        use std::fs::File;
        use std::io::BufReader;
        use std::path::Path;

        use afd_core::env::ProcessEnv;
        use afr_sandbox::{
            BubblewrapBackend, CGROUP_ROOT, HostPaths, Release, SELF_CGROUP_PATH, boot,
            delegated_root,
        };

        let own = File::open(SELF_CGROUP_PATH).map_err(|error| io_failed(&error))?;
        let cgroup_root =
            delegated_root(BufReader::new(own), Path::new(CGROUP_ROOT), RUNNER_SUBGROUP)
                .or_exit()?;
        let release = Release::signed_by_release(env!("CARGO_PKG_VERSION")).or_exit()?;
        let entry = std::env::current_exe().map_err(|error| io_failed(&error))?;
        let toolbox = home.toolbox();
        let backend = BubblewrapBackend {
            entry,
            release: &release,
            home: &toolbox,
            env: &ProcessEnv,
        };
        let paths = HostPaths {
            cgroup_root,
            state_dir: home.sandboxes(),
        };
        boot(backend, &paths).map(Self::from).or_exit()
    }

    /// No bubblewrap engine exists off Linux.
    ///
    /// # Errors
    /// Always: the first mechanism the host's probe found missing, already
    /// logged, as the exit status.
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn production(_home: &StorageHome) -> Result<Self, ExitCode> {
        let probe = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
        Err(cannot_sandbox(
            afr_sandbox::BubblewrapRequirements::missing(&probe)
                .unwrap_or(afr_sandbox::MECHANISM_BUBBLEWRAP),
        ))
    }

    /// An engine that builds no sandbox: each lease's tools run in a scratch
    /// directory under the storage home. A debug build alone has it, and the
    /// engine itself refuses to exist in a release build.
    ///
    /// # Errors
    /// The engine refused, already logged, as the exit status.
    #[cfg(debug_assertions)]
    pub(crate) fn unsandboxed(home: &StorageHome) -> Result<Self, ExitCode> {
        let engine = afr_sandbox::UnsandboxedEngine::new(home.sandboxes()).or_exit()?;
        let probe = afr_sandbox::probe(&afr_sandbox::ProbePaths::default());
        Ok(Self::new(engine, probe, None))
    }
}

#[cfg(any(target_os = "linux", debug_assertions))]
impl<E: Engine + 'static> From<afr_sandbox::Booted<E>> for Host {
    fn from(value: afr_sandbox::Booted<E>) -> Self {
        Self::new(
            value.started.engine,
            value.probe,
            Some(value.started.resources),
        )
    }
}

/// The agent loop every lease's turn runs on: every tool this runner hosts,
/// their egress through the guarded network, and the model providers the
/// shipped registry names.
///
/// # Errors
/// The failure, already logged, as the exit status.
pub(crate) fn agent() -> Result<afr_agent::Loop, ExitCode> {
    let network = afr_egress::Network::new().or_exit()?;
    let connector = afr_providers::Connector::new().or_exit()?;
    Ok(afr_agent::Loop::new(
        afr_tools::Catalog::hosted(Arc::new(network)),
        connector,
    ))
}

/// A step `run` cannot go on from: its failure, whichever crate raised it,
/// logged under its registry code and `run_failed`, as the exit status.
pub(crate) trait OrExit<T> {
    /// The value, or the failure logged and turned into the exit status.
    ///
    /// # Errors
    /// The step failed.
    fn or_exit(self) -> Result<T, ExitCode>;
}

impl<T, E: Coded> OrExit<T> for Result<T, E> {
    fn or_exit(self) -> Result<T, ExitCode> {
        self.map_err(|failure| stopped(failure.code(), &failure.told()))
    }
}

/// Logs an input/output failure the runner's own crates did not raise.
pub(crate) fn io_failed(failure: &std::io::Error) -> ExitCode {
    stopped(error_code::INTERNAL_OPERATION_FAILED, &failure.to_string())
}

/// Logs a host that lacks `missing`, which every sandbox needs.
#[cfg(not(target_os = "linux"))]
fn cannot_sandbox(missing: &'static str) -> ExitCode {
    let error_code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let reason = CANNOT_SANDBOX;
    let event = EVENT_RUN_FAILED;
    tracing::error!(error_code, missing, reason, event);
    ExitCode::FAILURE
}

/// Logs why `run` stopped, `reason` being the failure's sentence and its
/// causes, and turns its code into the exit status.
fn stopped(code: ErrorCode, reason: &str) -> ExitCode {
    let error_code = code.as_str();
    let event = EVENT_RUN_FAILED;
    tracing::error!(error_code, reason, event);
    ExitCode::from(exit_status(code))
}

/// The status a failure coded `code` ends `run` with.
fn exit_status(code: ErrorCode) -> u8 {
    if code == error_code::RUN_INVALID_RUNNER_TOKEN {
        EXIT_TOKEN_REFUSED
    } else {
        EXIT_FAILED
    }
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
