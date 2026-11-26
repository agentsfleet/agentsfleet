//! What the lane needs before any trial runs, and the engine every trial uses.

use std::fs;
use std::path::{Path, PathBuf};

use af_core::env::{EnvSource as _, ProcessEnv};
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_engine::{
    EgressPlan, Engine, Generation, Limits, Network, ResourceFacts, Resources, SandboxSpec,
};
use afr_engine_bubblewrap::{BubblewrapConfig, BubblewrapEngine, ProbePaths, probe};
use afr_engine_firecracker::{FirecrackerConfig, FirecrackerEngine};
use afr_firewall::far::forwarding_on;
use afr_image::Manifest;
use libtest_mimic::Failed;

pub(crate) use self::boot::{main, serve};
use crate::release::Signer;

#[path = "lane_boot.rs"]
mod boot;

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The environment variable naming the toolbox source directory, which holds
/// the `manifest.txt` and `build.sh` the image was built from. It sits under
/// `scripts/`, outside this crate, so the make lane names it.
const SOURCE_VARIABLE: &str = "AFR_TOOLBOX_SOURCE";
/// The unprivileged host user and group bubblewrap runs as, as on a host.
pub(crate) const SANDBOX_IDS: (u32, u32) = afr_engine_bubblewrap::SANDBOX_HOST_IDS;
/// The cap on user namespaces; zero means bubblewrap cannot start.
const MAX_USER_NAMESPACES: &str = "/proc/sys/user/max_user_namespaces";
/// Ubuntu's `AppArmor` switch that forbids unprivileged user namespaces.
const APPARMOR_USERNS: &str = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
/// What a host that does not forward IPv4 lacks: an allowlisted sandbox's
/// traffic is forwarded from its link, so the egress trials need it on.
const FORWARDING_OFF: &str = "forwarding: net.ipv4.ip_forward is not 1";
/// Where each run's runs and toolbox mount live; short, for socket paths.
pub(crate) const STATE_PREFIX: &str = "afr-lane-";
/// What the sandbox side says, on its error stream, when its command line
/// names no descriptors it can read.
pub(crate) const NOT_TOLD: &str = "the sandbox was not told its descriptors";
/// Where, under the lane's state, each run's directory is made; apart from
/// the toolbox mount, so no run name can land on it.
const RUNS_DIR: &str = "runs";
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

/// Everything a trial builds sandboxes with.
pub(crate) struct Lane {
    pub(crate) config: BubblewrapConfig,
    /// What the Firecracker engine boots from, over the same verified toolbox,
    /// when the lane holds it to the suite.
    pub(crate) firecracker: Option<FirecrackerConfig>,
    pub(crate) image: LaneImage,
    /// The key the lane's releases are signed with, and the manifest of the
    /// image it runs on.
    pub(crate) signer: Signer,
    pub(crate) manifest: Manifest,
    resources: Box<dyn Resources>,
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

    /// The engine the conformance suite holds to its trials: Firecracker when
    /// the lane was told so, bubblewrap otherwise.
    pub(crate) fn conformed(&self) -> Box<dyn Engine> {
        match self.firecracker.clone() {
            Some(config) => Box::new(
                FirecrackerEngine::new(config).unwrap_or_else(|refused| unreachable!("{refused}")),
            ),
            None => Box::new(self.engine()),
        }
    }

    /// The Firecracker engine, for a trial only it answers.
    pub(crate) fn microvms(&self) -> Result<FirecrackerEngine, Failed> {
        let config = self
            .firecracker
            .clone()
            .ok_or_else(|| Failed::from("the lane was not told to boot Firecracker"))?;
        Ok(FirecrackerEngine::new(config)?)
    }

    /// Where the sandbox a trial names `name` keeps its directory.
    pub(crate) fn run_dir(&self, name: &str) -> PathBuf {
        self.state.path().join(RUNS_DIR).join(id_of(name).as_str())
    }
}

/// The sandbox identifier a trial names `name`: the same valid v7 identifier
/// for the same name, so a trial finds the cgroup and directory it built by
/// the name it used.
pub(crate) fn id_of(name: &str) -> Uuid7 {
    let digest = af_core::etag::sha256_hex(name.as_bytes());
    Uuid7::parse(&format!("0198f0c2-7a3e-7c1d-9b2e-{}", &digest[..12]))
        .unwrap_or_else(|refused| unreachable!("{refused}"))
}

/// Plan `generation` reaching `network`.
pub(crate) const fn plan(generation: u64, network: Network<'_>) -> EgressPlan<'_> {
    EgressPlan {
        generation: Generation(generation),
        network,
    }
}

/// A spec for `id` under `limits`, as bubblewrap grants them: every cap.
pub(crate) fn spec(id: &Uuid7, limits: Limits) -> SandboxSpec<'_> {
    let granted = ResourceFacts::ENFORCED
        .grant(SandboxEngine::Bubblewrap, limits)
        .unwrap_or_else(|refused| unreachable!("{refused}"));
    SandboxSpec::new(id, granted)
}

/// Every prerequisite this host lacks, named; empty when the lane can run.
pub(crate) fn missing(paths: &ProbePaths, toolbox: Option<&str>, root: bool) -> Vec<String> {
    let mut gaps = Vec::new();
    if !root {
        gaps.push("root: mounts and cgroups need it".to_owned());
    }
    if let Some(mechanism) = afr_engine_bubblewrap::BubblewrapRequirements::missing(&probe(paths)) {
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
    if !forwarding_on() {
        gaps.push(FORWARDING_OFF.to_owned());
    }
    if toolbox.is_none() {
        gaps.push(format!("{TOOLBOX_VARIABLE}: names no toolbox image"));
    }
    if toolbox_source().is_none() {
        gaps.push(format!(
            "{SOURCE_VARIABLE}: names no toolbox source directory"
        ));
    }
    gaps.extend(short_of_disk(Path::new(STATE_PARENT)));
    gaps
}

/// The toolbox source directory [`SOURCE_VARIABLE`] names, if it names one.
pub(crate) fn toolbox_source() -> Option<PathBuf> {
    ProcessEnv.get(SOURCE_VARIABLE).map(PathBuf::from)
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
