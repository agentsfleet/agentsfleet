//! Firecracker's boot against real delegated cgroups whose controllers start
//! disabled: it enables them, refuses a root that lacks one, and keeps the
//! kernel's own refusal of a populated root.

use std::error::Error as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use afr_cgroup::{CGROUP_PROCS, RunCgroup, SUBTREE_CONTROL};
use afr_engine::{Limits, REQUIRED_CONTROLLERS};
use afr_image::ToolboxHome;
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::expect;

const CONTROLLERS: &str = "cgroup.controllers";
const RUN: &str = "boot-run";
const CHILD: &str = "undelegated";
const TOOLBOX: &str = "toolbox";
const CPU: &str = "cpu";
const UNMOUNTED: &str = "no toolbox mounted";
const CHILD_SLEEP_SECONDS: &str = "60";
/// The process cap [`Limits::default`] writes to a run's `pids.max`.
const DEFAULT_PIDS_MAX: &str = "512";

/// Each trial owns an empty cgroup below the lane's delegated root.
fn with_root(
    lane: &Lane,
    trial: impl FnOnce(&Path, &Path) -> Result<(), Failed>,
) -> Result<(), Failed> {
    let state = tempfile::tempdir_in("/var/tmp")?;
    let name = state.path().file_name().ok_or("a unique cgroup name")?;
    let root = lane.config.cgroup_root.join(name);
    fs::create_dir(&root)?;
    let result = trial(&root, state.path());
    fs::remove_dir(root)?;
    result
}

/// Whether a refused boot left no toolbox mounted under `state`.
fn nothing_mounted(state: &Path) -> Result<(), Failed> {
    let mounts = ToolboxHome::at(state.join(TOOLBOX)).mounts();
    let empty = !mounts.exists() || fs::read_dir(mounts)?.next().is_none();
    expect(empty, UNMOUNTED)
}

/// A root whose controllers start disabled boots, twice, with each enabled
/// and a run's cgroup limited under it; its parent is left as it was.
pub(crate) fn boots_disabled(lane: &Lane) -> Result<(), Failed> {
    with_root(lane, |root, state| {
        let available = fs::read_to_string(root.join(CONTROLLERS))?;
        expect(
            REQUIRED_CONTROLLERS
                .iter()
                .all(|name| available.split_whitespace().any(|s| s == *name)),
            "all controllers delegated",
        )?;
        expect(
            fs::read_to_string(root.join(SUBTREE_CONTROL))?
                .trim()
                .is_empty(),
            "controllers initially disabled",
        )?;
        let parent_before = fs::read_to_string(lane.config.cgroup_root.join(SUBTREE_CONTROL))?;
        // A second boot over the enabled root proves restart is idempotent.
        for _ in 0..2 {
            let host = lane.boot_over(root, state)??;
            let checks = (|| {
                expect(
                    host.probe.missing_controller().is_none(),
                    "boot reports every controller enabled",
                )?;
                let run = RunCgroup::create(root, RUN, &Limits::default())?;
                let limited = expect(
                    fs::read_to_string(root.join(RUN).join("pids.max"))?.trim() == DEFAULT_PIDS_MAX,
                    "the child enforces the run limit",
                );
                run.remove()?;
                limited
            })();
            drop(host.started.engine);
            host.started.resources.close()?;
            checks?;
        }
        expect(
            fs::read_to_string(lane.config.cgroup_root.join(SUBTREE_CONTROL))? == parent_before,
            "boot leaves the parent unchanged",
        )
    })
}

/// A root its parent delegated no controller to is refused naming `cpu`,
/// and no toolbox is mounted.
pub(crate) fn refuses_unavailable(lane: &Lane) -> Result<(), Failed> {
    with_root(lane, |root, state| {
        let child = root.join(CHILD);
        fs::create_dir(&child)?;
        let result = lane.boot_over(&child, state);
        fs::remove_dir(child)?;
        let refused = result?
            .err()
            .ok_or("an undelegated controller must stop boot")?;
        expect(
            refused.missing_mechanism() == Some(CPU),
            format!("missing cpu: {refused}"),
        )?;
        nothing_mounted(state)
    })
}

/// A root holding a process is refused with the kernel's `EBUSY`; boot never
/// moves the process, enables nothing and mounts no toolbox.
pub(crate) fn refuses_populated(lane: &Lane) -> Result<(), Failed> {
    with_root(lane, |root, state| {
        let mut child = Command::new("sleep").arg(CHILD_SLEEP_SECONDS).spawn()?;
        let result = (|| {
            fs::write(root.join(CGROUP_PROCS), child.id().to_string())?;
            let refused = lane
                .boot_over(root, state)?
                .err()
                .ok_or("a populated root must stop boot")?;
            let cause = refused
                .source()
                .and_then(|cause| cause.downcast_ref::<std::io::Error>());
            expect(
                cause.and_then(std::io::Error::raw_os_error) == Some(libc::EBUSY),
                format!("activation preserves kernel EBUSY: {refused}"),
            )?;
            expect(
                fs::read_to_string(root.join(CGROUP_PROCS))?.trim() == child.id().to_string(),
                "boot never moves the process",
            )?;
            expect(
                fs::read_to_string(root.join(SUBTREE_CONTROL))?
                    .trim()
                    .is_empty(),
                "failed activation enables nothing",
            )?;
            nothing_mounted(state)
        })();
        child.kill()?;
        child.wait()?;
        result
    })
}
