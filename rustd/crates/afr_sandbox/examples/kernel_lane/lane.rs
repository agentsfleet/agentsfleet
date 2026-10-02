//! What the lane needs before any trial runs, and the engine every trial uses.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use afd_core::env::LOG_LEVEL_VAR;
use afr_sandbox::bubblewrap::SANDBOX_SUBCOMMAND;
use afr_sandbox::{
    BubblewrapConfig, BubblewrapEngine, HostTools, ProbePaths, Toolbox, ToolboxImage, probe,
};

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The delegated cgroup every lease's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// The controllers the lane delegates to each lease.
const CONTROLLERS: &str = "+cpu +io +memory +pids";
/// The cgroup v2 root, which must hand the controllers down first.
const CGROUP_V2_ROOT: &str = "/sys/fs/cgroup";
/// The unprivileged host user and group bubblewrap runs as: `nobody`.
pub(crate) const SANDBOX_IDS: (u32, u32) = (65_534, 65_534);
/// The file a cgroup enables its children's controllers in.
const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
/// The cap on user namespaces; zero means bubblewrap cannot start.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// Ubuntu's `AppArmor` switch that forbids unprivileged user namespaces.
const APPARMOR_USERNS: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// Where each run's leases and toolbox mount live; short, for socket paths.
const STATE_PREFIX: &str = "afr-lane-";
/// Where, under the lane's state, each lease's directory is made; apart from
/// the toolbox mount, so no lease name can land on it.
const LEASES_DIR: &str = "leases";
/// Where, under the lane's state, the toolbox is mounted.
const TOOLBOX_DIR: &str = "toolbox";
/// The lane binary's name where the sandbox binds it from.
const ENTRY_NAME: &str = "agentsfleet-runner";
/// Readable and executable by everyone, writable by nobody but root.
const ENTRY_MODE: u32 = 0o755;
/// The state directory: traversable, but listable and writable by root only.
const STATE_MODE: u32 = 0o711;
/// How long a sandbox may take to answer.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything a trial builds sandboxes with.
pub(crate) struct Lane {
    pub(crate) config: BubblewrapConfig,
    pub(crate) image: ToolboxImage,
    state: tempfile::TempDir,
}

impl Lane {
    /// The engine every trial uses.
    pub(crate) fn engine(&self) -> BubblewrapEngine {
        BubblewrapEngine::new(self.config.clone())
            .unwrap_or_else(|refused| unreachable!("{refused}"))
    }

    /// Where a lease's directory goes.
    pub(crate) fn lease_dir(&self, lease_id: &str) -> PathBuf {
        self.state.path().join(LEASES_DIR).join(lease_id)
    }
}

/// Every prerequisite this host lacks, named; empty when the lane can run.
pub(crate) fn missing(paths: &ProbePaths, toolbox: Option<&str>, root: bool) -> Vec<String> {
    let mut gaps = Vec::new();
    if !root {
        gaps.push("root: mounts and cgroups need it".to_owned());
    }
    if let Some(mechanism) = probe(paths).missing() {
        gaps.push(format!(
            "{mechanism}: the kernel lacks it or it is not delegated"
        ));
    }
    let namespaces = fs::read_to_string(MAX_USER_NAMESPACES).unwrap_or_default();
    if namespaces.trim().parse::<u64>().unwrap_or(0) == 0 {
        gaps.push(format!(
            "user namespaces: {MAX_USER_NAMESPACES} is zero or unreadable"
        ));
    }
    if fs::read_to_string(APPARMOR_USERNS).is_ok_and(|on| on.trim() == "1") {
        gaps.push(format!("user namespaces: {APPARMOR_USERNS} is 1"));
    }
    if toolbox.is_none() {
        gaps.push(format!("{TOOLBOX_VARIABLE}: names no toolbox image"));
    }
    gaps
}

/// Checks every prerequisite, builds the lane, and runs the trials.
pub(crate) fn main() -> ExitCode {
    let toolbox = std::env::var(TOOLBOX_VARIABLE).ok();
    let root = rustix::process::geteuid().is_root();
    // The controllers are delegated first, so the probe reads the cgroup the
    // leases are made under; a host that cannot delegate them is a gap too.
    let delegated = if root { delegate().ok() } else { None };
    let paths = ProbePaths {
        cgroup_root: delegated
            .clone()
            .unwrap_or_else(|| PathBuf::from(LANE_CGROUP)),
        ..ProbePaths::default()
    };
    let gaps = missing(&paths, toolbox.as_deref(), root);
    if !gaps.is_empty() {
        eprintln!("the kernel lane refuses to run, rather than skip; this host lacks:");
        for gap in &gaps {
            eprintln!("  - {gap}");
        }
        return ExitCode::FAILURE;
    }
    let lane = match build(toolbox.as_deref().map_or(Path::new(""), Path::new), paths) {
        Ok(lane) => lane,
        Err(reason) => {
            eprintln!("the kernel lane could not set up: {reason}");
            return ExitCode::FAILURE;
        }
    };
    let mut arguments = libtest_mimic::Arguments::from_args();
    arguments.test_threads = Some(1);
    let lane = Arc::new(lane);
    let conclusion = crate::trials::run(&arguments, &lane);
    // The toolbox is mounted inside the lane's state, which cannot be removed
    // while it is; a run that leaves nothing behind can run again.
    if let Err(left) = lane.config.toolbox.clone().unmount() {
        eprintln!("the lane's toolbox stayed mounted: {left}");
        return ExitCode::FAILURE;
    }
    conclusion.exit_code()
}

fn build(image: &Path, probe: ProbePaths) -> Result<Lane, String> {
    let image = ToolboxImage::verify(image).map_err(|error| error.to_string())?;
    let cgroup_root = probe.cgroup_root.clone();
    let state = tempfile::Builder::new()
        .prefix(STATE_PREFIX)
        .tempdir_in("/tmp")
        .map_err(|error| error.to_string())?;
    let runtime = crate::run::runtime();
    let tools = HostTools::default();
    let toolbox = runtime
        .block_on(Toolbox::mount(
            &image,
            &tools,
            &state.path().join(TOOLBOX_DIR),
        ))
        .map_err(|error| error.to_string())?;
    let entry = install_entry(state.path()).map_err(|error| error.to_string())?;
    let config = BubblewrapConfig {
        tools,
        probe,
        toolbox_digest: image.digest().to_owned(),
        toolbox,
        cgroup_root,
        state_dir: state.path().join(LEASES_DIR),
        entry,
        entry_args: vec![OsString::from(SANDBOX_SUBCOMMAND)],
        sandbox_ids: SANDBOX_IDS,
        log_level: std::env::var_os(LOG_LEVEL_VAR),
        ready_timeout: READY_TIMEOUT,
    };
    Ok(Lane {
        config,
        image,
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
    // The lease directories live beside it, so the directory itself must be
    // traversable from inside too.
    fs::set_permissions(state, fs::Permissions::from_mode(STATE_MODE))?;
    Ok(entry)
}

/// Makes the lane's own cgroup and delegates the lease controllers to it.
fn delegate() -> std::io::Result<PathBuf> {
    fs::write(Path::new(CGROUP_V2_ROOT).join(SUBTREE_CONTROL), CONTROLLERS)?;
    let root = PathBuf::from(LANE_CGROUP);
    if !root.exists() {
        fs::create_dir(&root)?;
    }
    fs::write(root.join(SUBTREE_CONTROL), CONTROLLERS)?;
    Ok(root)
}

/// The sandbox side: harden, then serve the executor until the lane hangs up.
pub(crate) fn serve() -> ExitCode {
    match afr_sandbox::serve_sandboxed() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the sandbox stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
