//! What the lane needs before any trial runs, and the engine every trial uses.

use std::fs;
use std::path::{Path, PathBuf};

use af_core::env::{EnvSource as _, ProcessEnv};
use af_core::id::Uuid7;
use afr_engine::{
    Booted, EgressPlan, Engine, Generation, HostPaths, HostProbe, Limits, Network, Resources,
    SandboxSpec,
};
use afr_engine_firecracker::{FirecrackerConfig, FirecrackerEngine};
use afr_firewall::far::forwarding_on;
use libtest_mimic::Failed;

pub(crate) use self::boot::main;
use crate::release::Signer;

#[path = "lane_boot.rs"]
mod boot;

/// The environment variable naming the toolbox image to run against.
pub(crate) const TOOLBOX_VARIABLE: &str = "AFR_TOOLBOX_IMAGE";
/// The environment variable naming the toolbox source directory, which holds
/// the `manifest.txt` and `build.sh` the image was built from. It sits under
/// `scripts/`, outside this crate, so the make lane names it.
const SOURCE_VARIABLE: &str = "AFR_TOOLBOX_SOURCE";
/// What a host that does not forward IPv4 lacks: an allowlisted sandbox's
/// traffic is forwarded from its link, so the egress trials need it on.
const FORWARDING_OFF: &str = "forwarding: net.ipv4.ip_forward is not 1";
/// Where each run's runs and toolbox mount live; short, for socket paths.
pub(crate) const STATE_PREFIX: &str = "afr-lane-";
/// Where, under the lane's state, each run's directory is made; apart from
/// the toolbox mount, so no run name can land on it.
const RUNS_DIR: &str = "runs";
/// Where, under a boot's state, the toolbox image is staged, kept and
/// mounted, as a host keeps it.
const TOOLBOX_DIR: &str = "toolbox";
/// Where the lane's state is made: on a disk, as a host's is. A tmpfs `/tmp`
/// would make every workspace image memory, so a disk fill would be a
/// memory fill whatever the loop device caches.
const STATE_PARENT: &str = "/var/tmp";
/// The free space under [`STATE_PARENT`] a run needs: the concurrent
/// exhaustion trial's four 1 GiB disks filled at once, the staged toolboxes
/// and their mounts, with room to spare. Short of it, that trial's fills end
/// in the host's I/O errors rather than each sandbox's own `ENOSPC`.
pub(crate) const STATE_FREE_BYTES_MIN: u64 = 6 << 30;
/// A sandbox's workspace image, in its directory on the host.
pub(crate) const WORKSPACE_IMAGE: &str = "workspace.img";
/// Bytes in a mebibyte, for the free-space gap's numbers.
const MIB: u64 = 1 << 20;

/// Everything a trial builds sandboxes with.
pub(crate) struct Lane {
    /// What the Firecracker engine boots from, over the toolbox the lane
    /// verified as a host boots.
    pub(crate) config: FirecrackerConfig,
    pub(crate) image: LaneImage,
    /// The key the lane's releases are signed with.
    pub(crate) signer: Signer,
    /// What the lane's boot measured of the host, as a heartbeat reports it.
    pub(crate) probe: HostProbe,
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
    /// The Firecracker engine every trial and the conformance suite use. A
    /// new one sweeps what an earlier one left in the lane's state, as a
    /// restarted runner's does.
    pub(crate) fn microvms(&self) -> Result<FirecrackerEngine, Failed> {
        Ok(FirecrackerEngine::new(self.config.clone())?)
    }

    /// Where the sandbox a trial names `name` keeps its directory: its jail,
    /// its workspace image and its bound namespace.
    pub(crate) fn run_dir(&self, name: &str) -> PathBuf {
        self.config.state_dir.join(id_of(name).as_str())
    }

    /// The cgroup of the sandbox a trial names `name`, its monitor's leaf
    /// beneath it.
    pub(crate) fn cgroup_of(&self, name: &str) -> PathBuf {
        self.config.cgroup_root.join(id_of(name).as_str())
    }

    /// Boots Firecracker again as a host boots, over the delegated cgroup
    /// `cgroup_root`, with the lane's image staged and its runs kept under
    /// `state`: a second boot, as a restarted runner's. The inner result is
    /// the boot's own verdict.
    pub(crate) fn boot_over(
        &self,
        cgroup_root: &Path,
        state: &Path,
    ) -> Result<afr_engine::Result<Booted<FirecrackerEngine>>, Failed> {
        let paths = HostPaths {
            cgroup_root: cgroup_root.to_owned(),
            state_dir: state.join(RUNS_DIR),
        };
        boot::firecracker(
            &self.signer,
            (self.image.path(), self.image.digest()),
            &state.join(TOOLBOX_DIR),
            &paths,
        )
    }
}

/// A spec for `id` under `limits`, as `engine` grants them, reaching
/// `network`.
pub(crate) fn spec<'a>(
    engine: &dyn Engine,
    id: &'a Uuid7,
    limits: Limits,
    network: Network<'a>,
) -> Result<SandboxSpec<'a>, Failed> {
    Ok(SandboxSpec::granted_by(engine, id, limits)?.with_egress(plan(1, network)))
}

/// A spec for `id` under the default limits, as `engine` grants them,
/// reaching nothing: what a pooled slot is built for, so a claim is served
/// warm.
pub(crate) fn isolated<'a>(engine: &dyn Engine, id: &'a Uuid7) -> Result<SandboxSpec<'a>, Failed> {
    spec(engine, id, Limits::default(), Network::Isolated)
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

/// Every prerequisite this host lacks before its engine's own, named; empty
/// when the lane can run.
pub(crate) fn missing(toolbox: Option<&str>, root: bool) -> Vec<String> {
    let mut gaps = Vec::new();
    if !root {
        gaps.push("root: mounts and cgroups need it".to_owned());
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
