//! What the lane needs before any trial runs, and the engine every trial uses.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use afd_core::env::ProcessEnv;
use afr_sandbox::{
    BubblewrapConfig, BubblewrapEngine, CGROUP_ROOT, KernelMounter, Manifest, ProbePaths,
    REQUIRED_CONTROLLERS, SUBTREE_CONTROL, Toolboxes, probe,
};
use libtest_mimic::Failed;

use crate::release::Signer;

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The delegated cgroup every lease's cgroup is made under.
const LANE_CGROUP: &str = "/sys/fs/cgroup/afr-kernel-lane";
/// The unprivileged host user and group bubblewrap runs as, as on a host.
pub(crate) const SANDBOX_IDS: (u32, u32) = afr_sandbox::SANDBOX_HOST_IDS;
/// The cap on user namespaces; zero means bubblewrap cannot start.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// Ubuntu's `AppArmor` switch that forbids unprivileged user namespaces.
const APPARMOR_USERNS: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// Whether the host forwards IPv4: an allowlisted sandbox's traffic is
/// forwarded from its link, so the egress trials need it on.
const IP_FORWARD: &str = "/proc/sys/net/ipv4/ip_forward";
/// Where each run's leases and toolbox mount live; short, for socket paths.
pub(crate) const STATE_PREFIX: &str = "afr-lane-";
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
pub(crate) const ENTRY_MODE: u32 = 0o755;
/// Where the lane's state is made: on a disk, as a host's is. A tmpfs `/tmp`
/// would make every workspace image memory, so a disk fill would be a
/// memory fill whatever the loop device caches.
const STATE_PARENT: &str = "/var/tmp";
/// The free space under [`STATE_PARENT`] a run needs: the concurrent
/// exhaustion trial's four 1 GiB disks filled at once, the staged toolbox and
/// the runner copy, with room to spare. Short of it, that trial's fills end in
/// the host's I/O errors rather than each sandbox's own `ENOSPC`.
pub(crate) const STATE_FREE_BYTES_MIN: u64 = 6 << 30;
/// Bytes in a mebibyte, for the free-space gap's numbers.
const MIB: u64 = 1 << 20;
/// The state directory: traversable, but listable and writable by root only.
const STATE_MODE: u32 = 0o711;

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
    if fs::read_to_string(IP_FORWARD).map_or(true, |on| on.trim() != "1") {
        gaps.push(format!("forwarding: {IP_FORWARD} is not 1"));
    }
    if toolbox.is_none() {
        gaps.push(format!("{TOOLBOX_VARIABLE}: names no toolbox image"));
    }
    gaps.extend(short_of_disk(Path::new(STATE_PARENT)));
    gaps
}

/// Why `parent` cannot hold a run's state, if it cannot: too little free, with
/// any lane state an earlier run left behind named so it can be cleared. This
/// run's own state is made after the check, so every such directory is an
/// earlier run's, or a concurrent one's, and is named rather than removed.
pub(crate) fn short_of_disk(parent: &Path) -> Option<String> {
    let free = match rustix::fs::statvfs(parent) {
        Ok(stat) => stat.f_bavail.saturating_mul(stat.f_frsize),
        Err(error) => return Some(format!("disk: {} is unreadable: {error}", parent.display())),
    };
    (free < STATE_FREE_BYTES_MIN).then(|| {
        let left: Vec<String> = fs::read_dir(parent)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(STATE_PREFIX))
            .collect();
        format!(
            "disk: {} has {} MiB free, under the {} MiB a run fills; lane state left by earlier runs: {left:?}",
            parent.display(),
            free / MIB,
            STATE_FREE_BYTES_MIN / MIB,
        )
    })
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
    // The constructor a runner host builds with, so the lane proves the
    // configuration production runs and not a second spelling of it.
    let config = BubblewrapConfig::for_host(
        toolbox,
        cgroup_root,
        state.path().join(LEASES_DIR),
        entry,
        &ProcessEnv,
    );
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
            // logging: the sandbox side starts no subscriber; stderr is its only report
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
