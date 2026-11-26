//! The engine: one microVM per run, built through the jailer on a host the
//! backend admitted.

use std::fs::{self, DirBuilder};
use std::io;
use std::net::Ipv4Addr;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::PathBuf;
use std::sync::Arc;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_engine::error::sandbox_exists;
use afr_engine::{
    Allowlist, Capabilities, EgressPlan, Engine, Environment, Generation, Granted, Limits, Network,
    Result, Root, Sandbox, SandboxSpec, Sweep, Swept,
};
use afr_image::{HostTools, Toolbox};

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
    /// An engine building from `config`, once it has swept what a previous
    /// runner left in its state directory.
    #[must_use]
    pub fn new(config: FirecrackerConfig) -> Self {
        let jails = JailIds::new(config.tuning.first_jail_id);
        let engine = Self {
            shared: Arc::new(Shared { config, jails }),
        };
        let _logged = engine.swept(Sweep::Leftovers);
        engine
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
                build::boot(&self.shared, kept).await
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
