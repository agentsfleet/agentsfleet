//! The engine: one microVM per run, built through the jailer on a host the
//! backend admitted.

use std::fs::{self, DirBuilder};
use std::io;
use std::net::Ipv4Addr;
use std::num::NonZeroU64;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_engine::error::sandbox_exists;
use afr_engine::{
    AdmissionBound, Allowlist, Capabilities, EgressPlan, Engine, Environment, FileSnapshots,
    Generation, Granted, Limits, Network, Overcommitted, Result, Root, Sandbox, SandboxSpec, Sweep,
    Swept,
};
use afr_image::{HostTools, Toolbox};

use crate::admission::{Admission, MEMINFO_PATH, disk_free, mem_available};
use crate::capabilities::CAPABILITIES;
use crate::config::FirecrackerTuning;
use crate::jail::{DIR_MODE, JailIds, Layout};
use crate::lock::StateLock;

mod backend;
mod build;
mod vm;

pub use self::backend::FirecrackerBackend;

/// The event a sandbox's build is logged under, and the one it failed under;
/// the supervisor, which knows what the run was for, logs the refusal itself.
const EVENT_PREPARE_STARTED: &str = "sandbox_prepare_started";
const EVENT_PREPARE_FAILED: &str = "sandbox_prepare_failed";
/// The event a monitor the host ended, which the engine did not stop, is
/// logged under.
const EVENT_SANDBOX_LOST: &str = "sandbox_lost";

/// Everything the engine builds microVMs from.
#[derive(Debug, Clone)]
pub struct FirecrackerConfig {
    /// What every boot runs and how it is paced.
    pub tuning: FirecrackerTuning,
    /// The mounted, verified toolbox whose loop device is every guest's
    /// read-only drive.
    pub toolbox: Arc<Toolbox>,
    /// The delegated cgroup each monitor's leaf is made under.
    pub cgroup_root: PathBuf,
    /// Where each sandbox's directory is made. Locked, then swept, when the
    /// engine is built.
    pub state_dir: PathBuf,
    /// The formatter each workspace image is made with.
    pub tools: HostTools,
}

/// What every sandbox of one engine shares.
#[derive(Debug)]
pub(crate) struct Shared {
    pub(crate) config: FirecrackerConfig,
    pub(crate) jails: Arc<JailIds>,
    pub(crate) admission: Admission,
    /// The SHA-256 of the initial file system every guest boots its agent
    /// from, read on the first build.
    guest_agent: OnceLock<Box<str>>,
    /// Held while the engine or a sandbox of it lives: no second engine
    /// sweeps or builds over this state directory.
    _state: StateLock,
}

impl Shared {
    /// Where sandbox `id`'s pieces live.
    pub(crate) fn layout(&self, id: &Uuid7) -> Layout {
        Layout::new(
            &self.config.state_dir,
            id.as_str(),
            &self.config.tuning.binaries.firecracker,
        )
    }
}

/// Builds one microVM per run.
#[derive(Debug)]
pub struct FirecrackerEngine {
    shared: Arc<Shared>,
}

impl FirecrackerEngine {
    /// An engine building from `config`, once it holds its state directory's
    /// lock and has swept what a previous runner left there, holding the
    /// host to the bound its `MemAvailable` and the free space under that
    /// directory make then.
    ///
    /// # Errors
    /// Another engine holds the state directory, which is refused naming its
    /// lock before anything is swept; `/proc/meminfo` would not read or
    /// states no `MemAvailable`; the state directory's file system would not
    /// state its free space; or the tuning's limits are not ones a microVM
    /// is granted.
    pub fn new(config: FirecrackerConfig) -> Result<Self> {
        let jails = JailIds::new(config.tuning.first_jail_id);
        // Before the sweep: a directory another engine holds is its live
        // sandboxes', never leftovers.
        let state = StateLock::take(&config.state_dir)?;
        // Before the bound: a leftover's monitor and image hold memory and
        // disk no sandbox of this engine will.
        let _logged = crate::sweep::sweep(Sweep::Leftovers, &config.state_dir, &config.cgroup_root);
        let available = Available {
            memory: mem_available(Path::new(MEMINFO_PATH))?,
            disk: disk_free(&config.state_dir)?,
        };
        let admission = Admission::new(bound(&config.tuning, available)?);
        Ok(Self {
            shared: Arc::new(Shared {
                config,
                jails,
                admission,
                guest_agent: OnceLock::new(),
                _state: state,
            }),
        })
    }

    /// What it builds microVMs from.
    #[must_use]
    pub fn config(&self) -> &FirecrackerConfig {
        &self.shared.config
    }

    fn swept(&self, scope: Sweep<'_>) -> Swept {
        let config = &self.shared.config;
        crate::sweep::sweep(scope, &config.state_dir, &config.cgroup_root)
    }

    /// Makes sandbox `spec.id`'s directory and workspace image, then boots it.
    async fn start(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        // First: a host at its bound formats no image and starts no monitor.
        let reservation = self.shared.admission.admit()?;
        let layout = self.shared.layout(spec.id);
        // Fresh, never reused: a directory already there belongs to a live or
        // parked sandbox, or to a leftover only a sweep may remove.
        DirBuilder::new()
            .mode(DIR_MODE)
            .create(layout.dir())
            .map_err(|error| match error.kind() {
                io::ErrorKind::AlreadyExists => sandbox_exists(spec.id),
                _other => error.into(),
            })?;
        let disk_bytes = spec.granted.limits().disk_bytes;
        let made = afr_image::format_image(
            &self.shared.config.tools,
            &layout.image(),
            disk_bytes,
            af_guest_wire::GUEST_USER,
        )
        .await;
        let booted = match made {
            Ok(()) => match self.kept(spec, &layout).await {
                Ok(kept) => build::boot(&self.shared, kept, reservation).await,
                Err(unread) => Err(unread),
            },
            Err(refused) => Err(refused.into()),
        };
        match booted {
            Ok(vm) => Ok(Box::new(vm)),
            Err(error) => {
                let _left_for_the_sweep = fs::remove_dir_all(layout.dir());
                Err(error)
            }
        }
    }

    /// What sandbox `spec.id` keeps across a park, its pieces under `layout`.
    async fn kept(&self, spec: SandboxSpec<'_>, layout: &Layout) -> Result<Kept> {
        let snapshots = FileSnapshots::new(spec.store.map(Arc::clone), layout.dir().to_owned());
        Ok(Kept {
            id: spec.id.clone(),
            granted: spec.granted,
            environment: self.environment().await?,
            plan: Plan::from(spec.egress),
            layout: layout.clone(),
            snapshots,
        })
    }

    /// What every guest runs on: our engine, the verified toolbox and the
    /// guest agent in the initial file system, each by its digest.
    async fn environment(&self) -> Result<Environment> {
        Ok(Environment {
            engine: SandboxEngine::Firecracker,
            root: Root::Toolbox {
                digest: self.shared.config.toolbox.digest().into(),
            },
            guest_agent: Some(self.guest_agent().await?),
        })
    }

    /// The SHA-256 of the initial file system the guest agent boots from,
    /// read once.
    async fn guest_agent(&self) -> Result<Box<str>> {
        if let Some(digest) = self.shared.guest_agent.get() {
            return Ok(digest.clone());
        }
        let initrd = tokio::fs::read(&self.shared.config.tuning.binaries.initrd).await?;
        let digest = af_core::etag::sha256_hex(&initrd).into_boxed_str();
        Ok(self.shared.guest_agent.get_or_init(|| digest).clone())
    }
}

#[async_trait::async_trait]
impl Engine for FirecrackerEngine {
    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        crate::grant(limits)
    }

    fn admission_bound(&self) -> Option<AdmissionBound> {
        Some(self.shared.admission.bound())
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        let sandbox_id = spec.id.as_str();
        let event = EVENT_PREPARE_STARTED;
        tracing::info!(sandbox_id, event);
        self.start(spec).await.inspect_err(|error| {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_PREPARE_FAILED;
            tracing::warn!(sandbox_id, error_code, reason, event);
        })
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        Ok(self.swept(scope))
    }
}

/// What the host has to back microVMs with, in bytes, read at boot.
#[derive(Debug, Clone, Copy)]
struct Available {
    /// The host's `MemAvailable`.
    memory: u64,
    /// The free space of the file system holding the state directory.
    disk: u64,
}

/// The bound `available` makes under `tuning`, the lesser of what memory and
/// disk admit. In memory, one microVM reserves its monitor's cgroup
/// throttle: its guest's granted memory and `tuning`'s monitor overhead
/// beside it. The cgroup's cap sits `tuning`'s write cache above for the
/// host's cache of unsaved image writes, which the kernel writes back under
/// the throttle, so admission does not reserve it. On disk, it reserves its
/// workspace image's granted size.
fn bound(tuning: &FirecrackerTuning, available: Available) -> Result<AdmissionBound> {
    let granted = crate::grant(tuning.limits)?;
    let limits = granted.limits();
    // Never zero: a granted guest holds at least one mebibyte.
    let reserved = build::monitor_limits(limits, tuning).memory_bytes;
    let reserved = NonZeroU64::new(reserved).unwrap_or(NonZeroU64::MAX);
    // A granted disk of no bytes holds none, so it never bounds the host.
    let image = NonZeroU64::new(limits.disk_bytes).unwrap_or(NonZeroU64::MIN);
    Ok(AdmissionBound::lesser(
        Overcommitted::of(available.memory, tuning.memory_overcommit, reserved),
        Overcommitted::of(available.disk, tuning.disk_overcommit, image),
    ))
}

/// Logs a monitor the host ended while the engine still held it running:
/// out of memory, most likely, under overcommit. `reason` is how it ended.
pub(crate) fn lost(reason: &str) {
    let engine: &str = SandboxEngine::Firecracker.into();
    let event = EVENT_SANDBOX_LOST;
    tracing::warn!(engine, reason, event);
}

/// What a sandbox keeps across a park: everything but its monitor.
#[derive(Debug)]
pub(crate) struct Kept {
    pub(crate) id: Uuid7,
    pub(crate) granted: Granted,
    pub(crate) environment: Environment,
    pub(crate) plan: Plan,
    pub(crate) layout: Layout,
    /// Its snapshots, published into the chunk store its build was handed,
    /// if any; their spools live in its directory, which only the runner
    /// reaches.
    pub(crate) snapshots: FileSnapshots,
}

/// The plan a sandbox was last held to, owned, so a resumed sandbox boots
/// held to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) generation: Generation,
    pub(crate) reach: Reach,
}

/// What a plan reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Reach {
    /// Every public address, through the tap.
    Public,
    /// Nothing: no tap.
    Isolated,
    /// The allowlist's addresses, through the tap.
    Allowed(Allowlist),
}

impl From<EgressPlan<'_>> for Plan {
    fn from(plan: EgressPlan<'_>) -> Self {
        Self {
            generation: plan.generation,
            reach: match plan.network {
                Network::Host => Reach::Public,
                Network::Isolated => Reach::Isolated,
                Network::Allowed(allowlist) => Reach::Allowed(allowlist.clone()),
            },
        }
    }
}

impl Plan {
    /// The addresses its tap's set must hold, sorted; none without a set.
    pub(crate) fn addresses(&self) -> Option<Vec<Ipv4Addr>> {
        let Reach::Allowed(allowlist) = &self.reach else {
            return None;
        };
        let mut addresses = allowlist.addresses().to_vec();
        addresses.sort_unstable();
        Some(addresses)
    }
}

#[cfg(test)]
mod tests;
