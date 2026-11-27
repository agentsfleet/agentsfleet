//! What `run` runs with on this host: the sandbox engine, the capability
//! report heartbeats carry, and the agent loop every run's turn runs on.
//!
//! On Linux the engine is Firecracker, built from the cgroup the service
//! manager delegated and the toolbox release a deploy staged
//! (`docs/architecture/runner_execution.md` §Toolbox). A debug build can be
//! told to build no sandbox at all, so a suite drives this binary end to end on
//! a host without KVM; a release build has no way to ask.

use std::process::ExitCode;
use std::sync::Arc;

use af_core::error_code::{self, Coded, ErrorCode};
use afr_engine::{Engine, HostProbe, Limits, WarmSlots};
use afr_supervisor::StorageHome;

/// The event every failure that stops `run` is logged under.
pub(crate) const EVENT_RUN_FAILED: &str = "run_failed";
/// The event the engine this host admitted at boot is logged under.
pub(crate) const EVENT_ENGINE_BOOTED: &str = "sandbox_engine_booted";
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

/// The engine and the facts `run` serves runs with.
pub(crate) struct Host {
    /// Builds each run's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// What this host's kernel can enforce, as every heartbeat states it.
    pub(crate) probe: HostProbe,
    /// Backend resources, released after serving and its runtime have ended.
    pub(crate) resources: Option<Box<dyn afr_engine::Resources>>,
    /// Sandboxes the engine starts ahead of their runs; none starts all cold.
    pub(crate) warm_pool: usize,
}

impl Host {
    /// A host running with `engine`, on a kernel `probe` describes, its
    /// sandboxes using `resources`. A release build off Linux builds no
    /// engine at all, so it has no caller there.
    #[cfg(any(target_os = "linux", all(debug_assertions, feature = "unsandboxed")))]
    fn new(
        engine: impl Engine + 'static,
        probe: HostProbe,
        resources: Option<Box<dyn afr_engine::Resources>>,
    ) -> Self {
        Self {
            engine: Box::new(engine),
            probe,
            resources,
            warm_pool: 0,
        }
    }

    /// Our Firecracker engine, booted over the release a deploy staged
    /// (`afr_engine::boot` with `FirecrackerBackend`), with `warm_pool`
    /// microVMs booted ahead of their runs: a host without a usable KVM
    /// device is refused naming `kvm`, and no other engine starts.
    ///
    /// # Errors
    /// The reason it cannot, already logged, as the exit status.
    #[cfg(target_os = "linux")]
    pub(crate) fn firecracker(
        home: &StorageHome,
        tuning: afr_engine_firecracker::FirecrackerTuning,
        warm_pool: usize,
    ) -> Result<Self, ExitCode> {
        use afr_engine::boot;
        use afr_engine_firecracker::FirecrackerBackend;

        let staged = Staged::read(home)?;
        let backend = FirecrackerBackend {
            release: &staged.release,
            home: &staged.toolbox,
            tuning,
        };
        let host = boot(backend, &staged.paths).map(Self::from).or_exit()?;
        Ok(Self { warm_pool, ..host })
    }

    /// No Firecracker engine exists off Linux.
    ///
    /// # Errors
    /// Always: the host has no KVM device, already logged, as the exit status.
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn firecracker(
        _home: &StorageHome,
        _tuning: afr_engine_firecracker::FirecrackerTuning,
        _warm_pool: usize,
    ) -> Result<Self, ExitCode> {
        Err(cannot_sandbox(afr_engine_firecracker::MECHANISM_KVM))
    }

    /// An engine that builds no sandbox: each run's tools run in a scratch
    /// directory under the storage home, its executor serving `sizes`. A
    /// debug build alone has it, and the engine itself refuses to exist in a
    /// release build.
    ///
    /// # Errors
    /// The engine refused, already logged, as the exit status.
    #[cfg(all(debug_assertions, feature = "unsandboxed"))]
    pub(crate) fn unsandboxed(
        home: &StorageHome,
        sizes: afr_executor::Sizes,
    ) -> Result<Self, ExitCode> {
        let engine = afr_engine_unsandboxed::UnsandboxedEngine::new(home.sandboxes())
            .or_exit()?
            .with_sizes(sizes);
        Ok(Self::new(engine, crate::probe::host(), None))
    }
}

/// What every engine on this host boots over: the cgroup the service manager
/// delegated, the state directory its sandboxes live in, the release the
/// runner is built to verify and the toolbox a deploy staged.
#[cfg(target_os = "linux")]
struct Staged {
    paths: afr_engine::HostPaths,
    release: afr_image::Release,
    toolbox: afr_image::ToolboxHome,
}

#[cfg(target_os = "linux")]
impl Staged {
    /// This host's, read from `home` and the process's own cgroup.
    ///
    /// # Errors
    /// The reason it cannot, already logged, as the exit status.
    fn read(home: &StorageHome) -> Result<Self, ExitCode> {
        use std::fs::File;
        use std::io::BufReader;
        use std::path::Path;

        use afr_cgroup::{CGROUP_ROOT, SELF_CGROUP_PATH, delegated_root};
        use afr_image::{Release, ToolboxHome};

        let own = File::open(SELF_CGROUP_PATH).map_err(|error| io_failed(&error))?;
        let cgroup_root =
            delegated_root(BufReader::new(own), Path::new(CGROUP_ROOT), RUNNER_SUBGROUP)
                .or_exit()?;
        let release = Release::signed_by_release(env!("CARGO_PKG_VERSION")).or_exit()?;
        let toolbox = ToolboxHome::open(home.toolbox()).map_err(|error| io_failed(&error))?;
        Ok(Self {
            paths: afr_engine::HostPaths {
                cgroup_root,
                state_dir: home.sandboxes(),
            },
            release,
            toolbox,
        })
    }
}

#[cfg(any(target_os = "linux", all(debug_assertions, feature = "unsandboxed")))]
impl<E: Engine + 'static> From<afr_engine::Booted<E>> for Host {
    fn from(value: afr_engine::Booted<E>) -> Self {
        Self::new(
            value.started.engine,
            value.probe,
            Some(value.started.resources),
        )
    }
}

impl Host {
    /// Logs the engine this host admitted, once, before any run: what it is,
    /// what `/dev/kvm` allows, and the most sandboxes its memory and disk
    /// admit when it reserves both for each. The worker count is the
    /// daemon's, and arrives with its policy after boot.
    pub(crate) fn announce(&self) {
        booted(
            self.engine.capabilities(),
            self.engine.admission_bound(),
            &self.probe,
        );
    }
}

/// Logs `sandbox_engine_booted` for an engine stating `capabilities` and
/// bounded by `bound`, on a host `probe` describes. Host facts only: no path,
/// token or tenant identifier.
fn booted(
    capabilities: afr_engine::Capabilities,
    bound: Option<afr_engine::AdmissionBound>,
    probe: &HostProbe,
) {
    let engine: &str = capabilities.engine.into();
    let kvm: &str = probe.kvm.into();
    let sandboxes_max = bound.map(|bound| bound.sandboxes_max);
    let event = EVENT_ENGINE_BOOTED;
    tracing::info!(engine, kvm, sandboxes_max, event);
}

/// `engine` with `slots` sandboxes granted `limits` started ahead of their
/// runs, and the pool's own handle for [`retire`]; `engine` alone when
/// `slots` is zero. Each pooled sandbox is one `engine` built, so it counts
/// against the engine's admission bound. Called inside the runtime, where the
/// pool's keeper runs.
pub(crate) fn pooled(
    engine: Box<dyn Engine>,
    slots: usize,
    limits: Limits,
) -> (Box<dyn Engine>, Option<Arc<WarmSlots>>) {
    if slots == 0 {
        return (engine, None);
    }
    let pool = Arc::new(WarmSlots::start(Arc::from(engine), slots, limits));
    (Box::new(Arc::clone(&pool)), Some(pool))
}

/// Destroys every pooled sandbox no run claimed, once the supervisor has let
/// go of the engine, and before the toolbox every guest boots from is
/// unmounted.
pub(crate) async fn retire(pool: Option<Arc<WarmSlots>>) {
    if let Some(pool) = pool.and_then(Arc::into_inner) {
        pool.shutdown().await;
    }
}

/// The agent loop every run's turn runs on: every tool this runner hosts,
/// their egress through the guarded network, and the model providers the
/// shipped registry names, each bounded as the runner's file says.
///
/// # Errors
/// The failure, already logged, as the exit status.
pub(crate) fn agent(settings: &crate::config::Settings) -> Result<afr_agent::Loop, ExitCode> {
    let network = af_egress::Network::limited(settings.egress).or_exit()?;
    let connector =
        af_providers::Connector::configured(settings.inference, settings.provider_retries)
            .or_exit()?;
    Ok(
        afr_agent::Loop::new(afr_tools::Catalog::hosted(Arc::new(network)), connector)
            .with_limits(settings.run_limits),
    )
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
