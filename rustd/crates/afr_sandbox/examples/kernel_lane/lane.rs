//! What the lane needs before any trial runs, and the engine every trial uses.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use afr_sandbox::bubblewrap::{SANDBOX_SOCKET, SANDBOX_SUBCOMMAND, SANDBOX_WORKSPACE};
use afr_sandbox::{
    BubblewrapConfig, BubblewrapEngine, HostTools, ProbePaths, Toolbox, ToolboxImage, probe,
};

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The delegated cgroup every lease's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// The controllers the lane delegates to each lease.
const CONTROLLERS: &str = "+cpu +memory +pids";
/// The file a cgroup enables its children's controllers in.
const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
/// The cap on user namespaces; zero means bubblewrap cannot start.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// Ubuntu's `AppArmor` switch that forbids unprivileged user namespaces.
const APPARMOR_USERNS: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// Where each run's leases and toolbox mount live; short, for socket paths.
const STATE_PREFIX: &str = "afr-lane-";
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
        self.state.path().join(lease_id)
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
    let gaps = missing(&ProbePaths::default(), toolbox.as_deref(), root);
    if !gaps.is_empty() {
        eprintln!("the kernel lane refuses to run, rather than skip; this host lacks:");
        for gap in &gaps {
            eprintln!("  - {gap}");
        }
        return ExitCode::FAILURE;
    }
    let lane = match build(toolbox.as_deref().map_or(Path::new(""), Path::new)) {
        Ok(lane) => lane,
        Err(reason) => {
            eprintln!("the kernel lane could not set up: {reason}");
            return ExitCode::FAILURE;
        }
    };
    let mut arguments = libtest_mimic::Arguments::from_args();
    arguments.test_threads = Some(1);
    crate::trials::run(&arguments, lane).exit_code()
}

fn build(image: &Path) -> Result<Lane, String> {
    let image = ToolboxImage::verify(image).map_err(|error| error.to_string())?;
    let cgroup_root = delegate().map_err(|error| error.to_string())?;
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
            &state.path().join("toolbox"),
        ))
        .map_err(|error| error.to_string())?;
    let entry = std::env::current_exe().map_err(|error| error.to_string())?;
    let config = BubblewrapConfig {
        tools,
        probe: ProbePaths::default(),
        toolbox,
        cgroup_root,
        state_dir: state.path().to_owned(),
        entry,
        entry_args: vec![OsString::from(SANDBOX_SUBCOMMAND)],
        ready_timeout: READY_TIMEOUT,
    };
    Ok(Lane {
        config,
        image,
        state,
    })
}

/// Makes the lane's own cgroup and delegates the lease controllers to it.
fn delegate() -> std::io::Result<PathBuf> {
    let root = PathBuf::from(LANE_CGROUP);
    if !root.exists() {
        fs::create_dir(&root)?;
    }
    fs::write(root.join(SUBTREE_CONTROL), CONTROLLERS)?;
    Ok(root)
}

/// The sandbox side: harden, then serve the executor until the lane hangs up.
pub(crate) fn serve() -> ExitCode {
    if let Err(refused) = afr_sandbox::harden() {
        eprintln!("the sandbox would not harden: {refused}");
        return ExitCode::FAILURE;
    }
    let served = crate::run::runtime().block_on(afr_executor::serve(
        Path::new(SANDBOX_SOCKET),
        Path::new(SANDBOX_WORKSPACE),
    ));
    match served {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the executor stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
