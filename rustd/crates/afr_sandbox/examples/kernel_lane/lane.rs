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
    BubblewrapConfig, BubblewrapEngine, CGROUP_ROOT, HostTools, KernelMounter, Manifest,
    ProbePaths, REQUIRED_CONTROLLERS, SUBTREE_CONTROL, Toolboxes, probe,
};
use libtest_mimic::Failed;

use crate::release::Signer;

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The delegated cgroup every lease's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// The unprivileged host user and group bubblewrap runs as: `nobody`.
pub(crate) const SANDBOX_IDS: (u32, u32) = (65_534, 65_534);
/// The cap on user namespaces; zero means bubblewrap cannot start.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// Ubuntu's `AppArmor` switch that forbids unprivileged user namespaces.
const APPARMOR_USERNS: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// Where each run's leases and toolbox mount live; short, for socket paths.
const STATE_PREFIX: &str = "afr-lane-";
/// Where, under the lane's state, each lease's directory is made; apart from
/// the toolbox mount, so no lease name can land on it.
const LEASES_DIR: &str = "leases";
/// Where, under the lane's state, the toolbox image is staged and kept, and
/// where it is mounted.
const TOOLBOX_DIR: &str = "toolbox";
const MOUNTS_DIR: &str = "mounts";
/// The lane binary's name where the sandbox binds it from.
const ENTRY_NAME: &str = "agentsfleet-runner";
/// Readable and executable by everyone, writable by nobody but root.
const ENTRY_MODE: u32 = 0o755;
/// Where the lane's state is made: on a disk, as a host's is. A tmpfs `/tmp`
/// would make every workspace image memory, so a disk fill would be a
/// memory fill whatever the loop device caches.
const STATE_PARENT: &str = "/var/tmp";
/// The state directory: traversable, but listable and writable by root only.
const STATE_MODE: u32 = 0o711;
/// How long a sandbox may take to answer.
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// Everything a trial builds sandboxes with.
pub(crate) struct Lane {
    pub(crate) config: BubblewrapConfig,
    pub(crate) image: LaneImage,
    /// The key the lane's releases are signed with, and the manifest of the
    /// image it runs on.
    pub(crate) signer: Signer,
    pub(crate) manifest: Manifest,
    toolboxes: Toolboxes<KernelMounter>,
    state: tempfile::TempDir,
}

/// The toolbox image the lane was given, and its digest.
pub(crate) struct LaneImage {
    path: PathBuf,
    digest: String,
}

impl LaneImage {
    /// The image as the build published it.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Its SHA-256.
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }
}

impl Lane {
    /// The engine every trial uses.
    pub(crate) fn engine(&self) -> BubblewrapEngine {
        BubblewrapEngine::new(self.config.clone(), &probe(&self.config.probe_paths()))
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
            let reason = reason.message().unwrap_or_default();
            eprintln!("the kernel lane could not set up: {reason}");
            return ExitCode::FAILURE;
        }
    };
    let mut arguments = libtest_mimic::Arguments::from_args();
    arguments.test_threads = Some(1);
    let lane = Arc::new(lane);
    let conclusion = crate::trials::run(&arguments, &lane);
    // The toolbox is mounted inside the lane's state, which cannot be removed
    // while it is; a run that leaves nothing behind can run again. Every
    // trial has ended, so the lane and its toolbox have one owner again.
    let Some(Lane {
        config,
        toolboxes,
        state,
        ..
    }) = Arc::into_inner(lane)
    else {
        eprintln!("a trial still holds the lane; its toolbox stays mounted");
        return ExitCode::FAILURE;
    };
    drop(config);
    let closed = toolboxes.close();
    drop(state);
    if let Err(stayed) = closed {
        eprintln!("the lane's toolbox stayed mounted: {stayed}");
        return ExitCode::FAILURE;
    }
    conclusion.exit_code()
}

/// Admits the lane's image the way a host does: its manifest checked against
/// the lane's key, the image staged, then admitted by descriptor.
fn build(image: &Path, paths: ProbePaths) -> Result<Lane, Failed> {
    let cgroup_root = paths.cgroup_root;
    let state = tempfile::Builder::new()
        .prefix(STATE_PREFIX)
        .tempdir_in(STATE_PARENT)?;
    let signer = Signer::new()?;
    let manifest = signer.manifest_beside(image)?;
    let mounter = KernelMounter::new(state.path().join(MOUNTS_DIR));
    let toolboxes = Toolboxes::open(state.path().join(TOOLBOX_DIR), mounter)?;
    let toolbox = toolboxes.admit(&manifest, image)?;
    let image = LaneImage {
        path: image.to_owned(),
        digest: toolbox.digest().to_owned(),
    };
    let entry = install_entry(state.path())?;
    let config = BubblewrapConfig {
        tools: HostTools::default(),
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
        signer,
        manifest,
        toolboxes,
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

/// Makes the lane's own cgroup and delegates the lease controllers to it,
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

/// The sandbox side: take the tenant leaf's descriptors the engine named,
/// harden, then serve the executor until the lane hangs up.
pub(crate) fn serve() -> ExitCode {
    let tenant = match afr_sandbox::TenantDescriptors::parse_from(std::env::args_os().skip(1)) {
        Ok(tenant) => tenant,
        Err(refused) => {
            eprintln!("the sandbox was not told its tenant leaf: {refused}");
            return ExitCode::FAILURE;
        }
    };
    match afr_sandbox::serve_sandboxed(tenant) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("the sandbox stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
