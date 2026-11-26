//! How the lane starts and ends: its prerequisites checked, its toolbox staged
//! and its engine built as a host boots, the trials run, and its state closed.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use af_core::env::{EnvSource as _, ProcessEnv};
use af_wire::runner::SandboxEngine;
use afr_cgroup::{CGROUP_ROOT, SUBTREE_CONTROL};
use afr_engine::{HostPaths, REQUIRED_CONTROLLERS, boot};
use afr_engine_bubblewrap::{BubblewrapBackend, ProbePaths};
use afr_image::ToolboxHome;
use libtest_mimic::Failed;

use super::{
    ENTRY_MODE, Lane, LaneImage, NOT_TOLD, RUNS_DIR, STATE_PARENT, STATE_PREFIX, TOOLBOX_VARIABLE,
    missing,
};
use crate::release::Signer;

/// The environment variable naming the engine the lane builds and holds to
/// the conformance suite: `make test-runner-kernel ENGINE=…`, bubblewrap when
/// unset.
const ENGINE_VARIABLE: &str = "AFR_KERNEL_LANE_ENGINE";
/// The delegated cgroup every run's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// Where, under the lane's state, the toolbox image is staged, kept and
/// mounted, as a host keeps it.
const TOOLBOX_DIR: &str = "toolbox";
/// The lane binary's name where the sandbox binds it from.
const ENTRY_NAME: &str = "agentsfleet-runner";
/// The state directory: traversable, but listable and writable by root only.
const STATE_MODE: u32 = 0o711;

/// The engine the lane holds to the conformance suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LaneEngine {
    Bubblewrap,
    Firecracker,
}

/// The engine [`ENGINE_VARIABLE`] names, bubblewrap when unset, or why it
/// names none the lane builds.
fn lane_engine() -> Result<LaneEngine, String> {
    let named = ProcessEnv.get(ENGINE_VARIABLE);
    match named.as_deref() {
        None => Ok(LaneEngine::Bubblewrap),
        Some(name) if name == <&str>::from(SandboxEngine::Bubblewrap) => Ok(LaneEngine::Bubblewrap),
        Some(name) if name == <&str>::from(SandboxEngine::Firecracker) => {
            Ok(LaneEngine::Firecracker)
        }
        Some(other) => Err(format!(
            "{ENGINE_VARIABLE}: names {other}; the lane builds bubblewrap or firecracker"
        )),
    }
}

/// The engine the lane builds, or every prerequisite this host lacks for it,
/// the engine's own gaps after the host's.
fn engine_or_gaps(
    paths: &ProbePaths,
    toolbox: Option<&str>,
    root: bool,
) -> Result<LaneEngine, Vec<String>> {
    let mut gaps = missing(paths, toolbox, root);
    let engine = lane_engine();
    match &engine {
        Ok(LaneEngine::Firecracker) => gaps.extend(crate::firecracker::gaps()),
        Ok(LaneEngine::Bubblewrap) => {}
        Err(gap) => gaps.push(gap.clone()),
    }
    if gaps.is_empty() {
        engine.map_err(|gap| vec![gap])
    } else {
        Err(gaps)
    }
}

/// Checks every prerequisite, builds the lane, and runs the trials.
pub(crate) fn main() -> ExitCode {
    let toolbox = ProcessEnv.get(TOOLBOX_VARIABLE);
    let root = rustix::process::geteuid().is_root();
    // The controllers are delegated first, so the probe reads the cgroup the
    // runs are made under; a host that cannot delegate them is a gap too.
    let delegated = if root { delegate().ok() } else { None };
    let paths = ProbePaths {
        cgroup_root: delegated.unwrap_or_else(|| PathBuf::from(LANE_CGROUP)),
        ..ProbePaths::default()
    };
    let engine = match engine_or_gaps(&paths, toolbox.as_deref(), root) {
        Ok(engine) => engine,
        Err(gaps) => {
            eprintln!("the kernel lane refuses to run, rather than skip; this host lacks:");
            for gap in &gaps {
                eprintln!("  - {gap}");
            }
            return ExitCode::FAILURE;
        }
    };
    let microvms = engine == LaneEngine::Firecracker;
    let lane = match build(
        toolbox.as_deref().map_or_else(|| Path::new(""), Path::new),
        paths,
        microvms,
    ) {
        Ok(lane) => lane,
        Err(reason) => {
            let reason = reason.message().unwrap_or_default();
            eprintln!("the kernel lane could not set up: {reason}");
            return ExitCode::FAILURE;
        }
    };
    let mut arguments = libtest_mimic::Arguments::from_args();
    arguments.test_threads = Some(1);
    if let Err(failed) = crate::trials::differences(&lane) {
        let reason = failed.message().unwrap_or_default();
        eprintln!("the lane's engine could not state what may differ: {reason}");
        return ExitCode::FAILURE;
    }
    let lane = Arc::new(lane);
    let conclusion = crate::trials::run(&arguments, &lane);
    if !closed(lane) {
        return ExitCode::FAILURE;
    }
    conclusion.exit_code()
}

/// Closes the lane once every trial has ended: whether its toolbox came
/// unmounted and its state was removed, each failure printed.
fn closed(lane: Arc<Lane>) -> bool {
    // The toolbox is mounted inside the lane's state, which cannot be removed
    // while it is; a run that leaves nothing behind can run again. Every
    // trial has ended, so the lane and its toolbox have one owner again.
    let Some(Lane {
        config,
        firecracker,
        resources,
        state,
        ..
    }) = Arc::into_inner(lane)
    else {
        eprintln!("a trial still holds the lane; its toolbox stays mounted");
        return false;
    };
    drop((config, firecracker));
    let closed = resources.close();
    drop(state);
    if let Err(stayed) = closed {
        eprintln!("the lane's toolbox stayed mounted: {stayed}");
        return false;
    }
    true
}

/// Boots the lane the way a host boots: the image staged as a deploy stages
/// it, its manifest checked against the lane's key, the image verified by
/// descriptor and the engine built over it, so the lane proves the boot
/// production runs and not a second spelling of it.
fn build(image: &Path, paths: ProbePaths, microvms: bool) -> Result<Lane, Failed> {
    let state = tempfile::Builder::new()
        .prefix(STATE_PREFIX)
        .tempdir_in(STATE_PARENT)?;
    let signer = Signer::new()?;
    let manifest = signer.manifest_beside(image)?;
    let home = ToolboxHome::open(state.path().join(TOOLBOX_DIR))?;
    signer.stage(image, manifest.sha256(), &home.incoming())?;
    let entry = install_entry(state.path())?;
    let release = signer.release()?;
    let backend = BubblewrapBackend {
        entry,
        release: &release,
        home: &home,
        env: &ProcessEnv,
        tuning: afr_engine_bubblewrap::EngineTuning::DEFAULT,
    };
    let host_paths = HostPaths {
        cgroup_root: paths.cgroup_root,
        state_dir: state.path().join(RUNS_DIR),
    };
    let booted = boot(backend, &host_paths)?;
    let image = LaneImage {
        path: image.to_owned(),
        digest: booted.started.engine.config().toolbox.digest().to_owned(),
    };
    let config = booted.started.engine.config().clone();
    let firecracker = microvms
        .then(|| crate::firecracker::config(state.path(), &host_paths.cgroup_root, &config));
    Ok(Lane {
        config,
        firecracker,
        image,
        signer,
        manifest,
        resources: booted.started.resources,
        state,
    })
}

/// Copies this binary where the sandbox can read it, as a runner host keeps
/// `agentsfleet-runner` under `/usr/local/bin`.
///
/// Root inside the sandbox's user namespace holds no capability over a file
/// whose owner is unmapped there, so the build's own output, under a user's
/// `0750` home, cannot be bound in.
fn install_entry(state: &Path) -> std::io::Result<PathBuf> {
    let entry = state.join(ENTRY_NAME);
    fs::copy(std::env::current_exe()?, &entry)?;
    fs::set_permissions(&entry, fs::Permissions::from_mode(ENTRY_MODE))?;
    // The run directories live beside it, so the directory itself must be
    // traversable from inside too.
    fs::set_permissions(state, fs::Permissions::from_mode(STATE_MODE))?;
    Ok(entry)
}

/// Makes the lane's own cgroup and delegates the run controllers to it,
/// from the cgroup v2 root down.
fn delegate() -> std::io::Result<PathBuf> {
    let enable = REQUIRED_CONTROLLERS
        .map(|controller| format!("+{controller}"))
        .join(" ");
    fs::write(Path::new(CGROUP_ROOT).join(SUBTREE_CONTROL), &enable)?;
    let root = PathBuf::from(LANE_CGROUP);
    if !root.exists() {
        fs::create_dir(&root)?;
    }
    fs::write(root.join(SUBTREE_CONTROL), &enable)?;
    Ok(root)
}

/// The sandbox side: take the descriptors and sizes the engine named,
/// harden, then serve the executor until the lane hangs up.
pub(crate) fn serve() -> ExitCode {
    let entry = match afr_guest::Entry::parse_from(std::env::args_os().skip(1)) {
        Ok(entry) => entry,
        Err(refused) => {
            // the sandbox side starts no subscriber; stderr is its only report
            eprintln!("{NOT_TOLD}: {refused}");
            return ExitCode::FAILURE;
        }
    };
    match afr_guest::serve_sandboxed(entry) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the sandbox stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
