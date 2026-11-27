//! How the lane starts and ends: its prerequisites checked, its toolbox staged
//! and its engine booted as a host boots, the trials run, and its state closed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use af_core::env::{EnvSource as _, ProcessEnv};
use af_wire::runner::SandboxEngine;
use afr_cgroup::{CGROUP_ROOT, SUBTREE_CONTROL};
use afr_engine::{HostPaths, REQUIRED_CONTROLLERS, boot};
use afr_engine_firecracker::FirecrackerBackend;
use afr_image::ToolboxHome;
use libtest_mimic::Failed;

use super::{Lane, LaneImage, RUNS_DIR, STATE_PARENT, STATE_PREFIX, TOOLBOX_VARIABLE, missing};
use crate::release::Signer;

/// The environment variable naming the engine the lane builds and holds to
/// the conformance suite: `make test-runner-kernel ENGINE=…`, Firecracker
/// when unset.
const ENGINE_VARIABLE: &str = "AFR_KERNEL_LANE_ENGINE";
/// The delegated cgroup every run's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// Where, under the lane's state, the toolbox image is staged, kept and
/// mounted, as a host keeps it.
const TOOLBOX_DIR: &str = "toolbox";

/// Why [`ENGINE_VARIABLE`] names no engine the lane builds, if it does not:
/// unset or `firecracker` is the one it builds.
fn engine_gap() -> Option<String> {
    let named = ProcessEnv.get(ENGINE_VARIABLE)?;
    (named != <&str>::from(SandboxEngine::Firecracker))
        .then(|| format!("{ENGINE_VARIABLE}: names {named}; the lane builds firecracker"))
}

/// Every prerequisite this host lacks, the engine's own after the host's.
fn gaps(toolbox: Option<&str>, root: bool) -> Vec<String> {
    let mut gaps = missing(toolbox, root);
    gaps.extend(crate::firecracker::gaps());
    gaps.extend(engine_gap());
    gaps
}

/// Checks every prerequisite, builds the lane, and runs the trials.
pub(crate) fn main() -> ExitCode {
    let toolbox = ProcessEnv.get(TOOLBOX_VARIABLE);
    let root = rustix::process::geteuid().is_root();
    // The controllers are delegated first, so the engine boots over the
    // cgroup the runs are made under; a host that cannot delegate them is a
    // gap its boot names.
    let delegated = if root { delegate().ok() } else { None };
    let cgroup_root = delegated.unwrap_or_else(|| PathBuf::from(LANE_CGROUP));
    let gaps = gaps(toolbox.as_deref(), root);
    if !gaps.is_empty() {
        eprintln!("the kernel lane refuses to run, rather than skip; this host lacks:");
        for gap in &gaps {
            eprintln!("  - {gap}");
        }
        return ExitCode::FAILURE;
    }
    let image = toolbox.as_deref().map_or_else(|| Path::new(""), Path::new);
    let lane = match build(image, cgroup_root) {
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
        resources,
        state,
        ..
    }) = Arc::into_inner(lane)
    else {
        eprintln!("a trial still holds the lane; its toolbox stays mounted");
        return false;
    };
    drop(config);
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
fn build(image: &Path, cgroup_root: PathBuf) -> Result<Lane, Failed> {
    let state = tempfile::Builder::new()
        .prefix(STATE_PREFIX)
        .tempdir_in(STATE_PARENT)?;
    let signer = Signer::new()?;
    let manifest = signer.manifest_beside(image)?;
    let home = ToolboxHome::open(state.path().join(TOOLBOX_DIR))?;
    signer.stage(image, manifest.sha256(), &home.incoming())?;
    let release = signer.release()?;
    let backend = FirecrackerBackend {
        release: &release,
        home: &home,
        tuning: crate::firecracker::tuning(),
    };
    let host_paths = HostPaths {
        cgroup_root,
        state_dir: state.path().join(RUNS_DIR),
    };
    let booted = boot(backend, &host_paths)?;
    let config = booted.started.engine.config().clone();
    let image = LaneImage {
        path: image.to_owned(),
        digest: config.toolbox.digest().to_owned(),
    };
    Ok(Lane {
        config,
        image,
        signer,
        resources: booted.started.resources,
        state,
    })
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
