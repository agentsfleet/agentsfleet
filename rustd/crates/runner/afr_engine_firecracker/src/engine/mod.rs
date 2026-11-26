//! The engine: one microVM per run, built through the jailer on a host the
//! backend admitted.

use std::fs::{self, DirBuilder};
use std::io;
use std::net::Ipv4Addr;
use std::num::NonZeroU64;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_engine::error::sandbox_exists;
use afr_engine::{
    Allowlist, Capabilities, EgressPlan, Engine, Environment, Generation, Granted, Limits,
    MemoryBound, Network, Result, Root, Sandbox, SandboxSpec, Sweep, Swept,
};
use afr_image::{HostTools, Toolbox};

use crate::admission::{Admission, MEMINFO_PATH, mem_available};
use crate::capabilities::CAPABILITIES;
use crate::config::FirecrackerTuning;
use crate::jail::{DIR_MODE, JailIds, Layout};

mod backend;
mod build;
mod vm;

pub use self::backend::FirecrackerBackend;

/// The user and group a guest's executor and every tenant process run as,
/// inside the guest: the toolbox's sandbox user, as bubblewrap maps it.
pub(crate) const GUEST_USER: (u32, u32) = (1000, 1000);
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
    /// Where each sandbox's directory is made. Swept when the engine is built.
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
    /// An engine building from `config`, holding the host to the memory
    /// bound its `MemAvailable` makes now, once it has swept what a previous
    /// runner left in its state directory.
    ///
    /// # Errors
    /// `/proc/meminfo` would not read or states no `MemAvailable`, or the
    /// tuning's limits are not ones a microVM is granted.
    pub fn new(config: FirecrackerConfig) -> Result<Self> {
        let available = mem_available(Path::new(MEMINFO_PATH))?;
        let admission = Admission::new(memory_bound(&config.tuning, available)?);
        let jails = JailIds::new(config.tuning.first_jail_id);
        let engine = Self {
            shared: Arc::new(Shared {
                config,
                jails,
                admission,
            }),
        };
        let _logged = engine.swept(Sweep::Leftovers);
        Ok(engine)
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
        fs::create_dir_all(&self.shared.config.state_dir)?;
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
            GUEST_USER,
        )
        .await;
        let booted = match made {
            Ok(()) => {
                let kept = Kept {
                    id: spec.id.clone(),
                    granted: spec.granted,
                    environment: self.environment(),
                    plan: Plan::from(spec.egress),
                    layout: layout.clone(),
                };
                build::boot(&self.shared, kept, reservation).await
            }
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

    fn environment(&self) -> Environment {
        Environment {
            engine: SandboxEngine::Firecracker,
            root: Root::Toolbox {
                digest: self.shared.config.toolbox.digest().into(),
            },
        }
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

    fn memory_bound(&self) -> Option<MemoryBound> {
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

/// The bound `available` bytes make under `tuning`: one microVM reserves what
/// its monitor's cgroup holds, its guest's granted memory and
/// `SANDBOX_MEMORY_RESERVE_BYTES` (64 MiB) beside it, which the kernel holds
/// the monitor to. The spike measured a 2 GiB guest's idle monitor at
/// 110.9 MiB resident with 46.2 MiB used inside the guest
/// (`docs/v2/reviews/m258-firecracker-spike.md`), about 64.7 MiB its own.
fn memory_bound(tuning: &FirecrackerTuning, available: u64) -> Result<MemoryBound> {
    let granted = crate::grant(tuning.limits)?;
    let reserved = build::monitor_limits(granted.limits()).memory_bytes;
    // Never zero: the reserve alone is 64 MiB.
    let reserved = NonZeroU64::new(reserved).unwrap_or(NonZeroU64::MAX);
    Ok(MemoryBound::overcommitted(
        available,
        tuning.memory_overcommit,
        reserved,
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
