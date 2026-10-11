//! Boot against real delegated cgroups whose controllers start disabled.

use std::error::Error as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use afd_core::env::ProcessEnv;
use afr_sandbox::{
    Booted, BubblewrapBackend, BubblewrapEngine, HostPaths, LeaseCgroup, Limits,
    REQUIRED_CONTROLLERS, SUBTREE_CONTROL, ToolboxHome, probe,
};
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::expect;

const CONTROLLERS: &str = "cgroup.controllers";
const PROCS: &str = "cgroup.procs";
const LEASE: &str = "boot-lease";
const CHILD: &str = "undelegated";
const TOOLBOX: &str = "toolbox";
const CPU: &str = "cpu";
const UNMOUNTED: &str = "no toolbox mounted";
const CHILD_SLEEP_SECONDS: &str = "60";

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

fn boot(lane: &Lane, root: &Path, state: &Path) -> Result<Booted<BubblewrapEngine>, Failed> {
    let home = ToolboxHome::open(state.join(TOOLBOX))?;
    lane.signer
        .stage(lane.image.path(), lane.image.digest(), &home.incoming())?;
    let leases = state.join("leases");
    fs::create_dir_all(&leases)?;
    let release = lane.signer.release()?;
    let backend = BubblewrapBackend {
        entry: std::env::current_exe()?,
        release: &release,
        home: &home,
        env: &ProcessEnv,
    };
    Ok(afr_sandbox::boot(
        backend,
        &HostPaths {
            cgroup_root: root.to_owned(),
            state_dir: leases,
        },
    )?)
}

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
        let mut paths = lane.config.probe_paths();
        root.clone_into(&mut paths.cgroup_root);
        expect(
            afr_sandbox::BubblewrapRequirements::missing(&probe(&paths)) == Some(CPU),
            "the original startup refusal",
        )?;
        // A second boot over the enabled root proves restart is idempotent.
        for _ in 0..2 {
            let host = boot(lane, root, state)?;
            let checks = (|| {
                expect(
                    afr_sandbox::BubblewrapRequirements::missing(&host.probe).is_none(),
                    "boot reports an enforceable host",
                )?;
                let lease = LeaseCgroup::create(root, LEASE, &Limits::default())?;
                let limited = expect(
                    fs::read_to_string(root.join(LEASE).join("pids.max"))?.trim() == "512",
                    "the child enforces the lease limit",
                );
                lease.remove()?;
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

pub(crate) fn refuses_unavailable(lane: &Lane) -> Result<(), Failed> {
    with_root(lane, |root, state| {
        let child = root.join(CHILD);
        fs::create_dir(&child)?;
        let home = ToolboxHome::open(state.join(TOOLBOX))?;
        let release = lane.signer.release()?;
        let backend = BubblewrapBackend {
            entry: std::env::current_exe()?,
            release: &release,
            home: &home,
            env: &ProcessEnv,
        };
        let result = afr_sandbox::boot(
            backend,
            &HostPaths {
                cgroup_root: child.clone(),
                state_dir: state.to_owned(),
            },
        );
        fs::remove_dir(child)?;
        let error = result
            .err()
            .ok_or("an undelegated controller must stop boot")?;
        expect(
            error.missing_mechanism() == Some(CPU),
            format!("missing cpu: {error}"),
        )?;
        expect(fs::read_dir(home.mounts())?.next().is_none(), UNMOUNTED)
    })
}

pub(crate) fn refuses_populated(lane: &Lane) -> Result<(), Failed> {
    with_root(lane, |root, state| {
        let home = ToolboxHome::open(state.join(TOOLBOX))?;
        let release = lane.signer.release()?;
        let entry = std::env::current_exe()?;
        let mut child = Command::new("sleep").arg(CHILD_SLEEP_SECONDS).spawn()?;
        let result = (|| {
            fs::write(root.join(PROCS), child.id().to_string())?;
            let backend = BubblewrapBackend {
                entry,
                release: &release,
                home: &home,
                env: &ProcessEnv,
            };
            let error = afr_sandbox::boot(
                backend,
                &HostPaths {
                    cgroup_root: root.to_owned(),
                    state_dir: state.to_owned(),
                },
            )
            .err()
            .ok_or("a populated root must stop boot")?;
            let cause = error
                .source()
                .and_then(|cause| cause.downcast_ref::<std::io::Error>());
            expect(
                cause.and_then(std::io::Error::raw_os_error) == Some(libc::EBUSY),
                format!("activation preserves kernel EBUSY: {error}"),
            )?;
            expect(
                fs::read_to_string(root.join(PROCS))?.trim() == child.id().to_string(),
                "boot never moves the process",
            )?;
            expect(
                fs::read_to_string(root.join(SUBTREE_CONTROL))?
                    .trim()
                    .is_empty(),
                "failed activation enables nothing",
            )?;
            expect(fs::read_dir(home.mounts())?.next().is_none(), UNMOUNTED)
        })();
        child.kill()?;
        child.wait()?;
        result
    })
}
