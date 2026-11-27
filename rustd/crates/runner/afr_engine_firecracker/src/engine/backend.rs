//! Firecracker's boot admission: the host it needs, the toolbox it verifies,
//! and the resources it holds until every microVM is gone.
//!
//! A host whose KVM device a monitor cannot open is refused naming `kvm`, and
//! one whose packet filter cannot hold a guest to its egress naming
//! `nftables`. The runner then starts no engine at all: there is no fallback
//! to another, and no guest runs behind a filter that is not there.

use std::path::Path;

use af_core::error_code::{Coded as _, Logged};
use afr_engine::error::refused;
use afr_engine::{
    Backend, HostPaths, HostProbe, MECHANISM_NFTABLES, MECHANISM_TOOLBOX_FILESYSTEM, Resources,
    Result, Started,
};
use afr_image::{HostTools, KernelMounter, MountedToolboxes, Release, ToolboxHome, Toolboxes};

use super::{FirecrackerConfig, FirecrackerEngine};
use crate::config::FirecrackerTuning;

/// The event a host whose egress sweep failed is logged under.
const EVENT_SWEEP_FAILED: &str = "egress_sweep_failed";

/// The release and tuning the shipping Firecracker backend boots with.
#[derive(Debug)]
pub struct FirecrackerBackend<'a> {
    /// Identity that verifies the staged toolbox.
    pub release: &'a Release,
    /// Images and mount locations supplied by the storage home.
    pub home: &'a ToolboxHome,
    /// The binaries every boot runs, and how each is paced.
    pub tuning: FirecrackerTuning,
}

impl Backend for FirecrackerBackend<'_> {
    type Engine = FirecrackerEngine;

    /// Enables the delegated controllers and reads what a microVM needs.
    fn measure(&self, paths: &HostPaths) -> Result<HostProbe> {
        afr_cgroup::enable_controllers(&paths.cgroup_root)?;
        Ok(HostProbe {
            landlock: afr_guest::landlock_enforceable()
                && afr_guest::landlock_listed(Path::new(afr_guest::LSM_PATH)),
            seccomp: afr_guest::seccomp_offered(Path::new(afr_guest::SECCOMP_ACTIONS_PATH)),
            cgroup_controllers: afr_cgroup::delegated_controllers(&paths.cgroup_root),
            kvm: crate::probe(Path::new(crate::KVM_PATH)),
            toolbox_filesystem: afr_image::toolbox_filesystem(Path::new(
                afr_image::FILESYSTEMS_PATH,
            )),
            workspace_direct_io: Some(afr_image::direct_io(&paths.state_dir)),
            egress: afr_firewall::enforceable(),
        })
    }

    fn validate(&self, host: &HostProbe) -> Result<()> {
        admitted(host)
    }

    fn start(self, paths: &HostPaths, _admitted: &HostProbe) -> Result<Started<Self::Engine>> {
        let toolboxes =
            Toolboxes::open(self.home.images(), KernelMounter::new(self.home.mounts()))?;
        let toolbox = toolboxes.verify_incoming(self.release, &self.home.incoming())?;
        let resources = Mounted(MountedToolboxes::from(toolboxes));
        // Before either sweep: a second runner on this host is refused
        // here, before it removes the first one's links or hands out its
        // jail users.
        afr_firewall::own_host()?;
        if let Err(error) = afr_firewall::sweep() {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_SWEEP_FAILED;
            tracing::warn!(error_code, reason, event);
        }
        let engine = FirecrackerEngine::new(FirecrackerConfig {
            tuning: self.tuning,
            toolbox,
            cgroup_root: paths.cgroup_root.clone(),
            state_dir: paths.state_dir.clone(),
            tools: HostTools::default(),
        })?;
        Ok(Started {
            engine,
            resources: Box::new(resources),
        })
    }
}

/// Whether Firecracker boots on `host`: a usable KVM device first, then the
/// toolbox's file system, every controller a monitor's leaf is limited
/// through, and a packet filter that built and removed an egress scope, so
/// no guest runs on a host that cannot hold it to its plan.
///
/// # Errors
/// The first of those the host lacks, named.
fn admitted(host: &HostProbe) -> Result<()> {
    crate::admit(host.kvm)?;
    if !host.toolbox_filesystem {
        return Err(refused(MECHANISM_TOOLBOX_FILESYSTEM));
    }
    if let Some(missing) = host.missing_controller() {
        return Err(refused(missing));
    }
    if !host.egress {
        return Err(refused(MECHANISM_NFTABLES));
    }
    Ok(())
}

/// The verified toolboxes every guest's drive is, mounted until the host's
/// engine and microVMs are gone.
#[derive(Debug)]
struct Mounted(MountedToolboxes<KernelMounter>);

impl Resources for Mounted {
    fn close(self: Box<Self>) -> Result<()> {
        Ok(self.0.close()?)
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use af_wire::runner::Kvm;
    use afr_engine::{HostProbe, REQUIRED_CONTROLLERS};

    use super::admitted;

    /// A host with everything a microVM needs.
    fn host() -> HostProbe {
        HostProbe {
            landlock: true,
            seccomp: true,
            cgroup_controllers: REQUIRED_CONTROLLERS.map(str::to_owned).into(),
            kvm: Kvm::Usable,
            toolbox_filesystem: true,
            workspace_direct_io: None,
            egress: true,
        }
    }

    /// A host that cannot hold a guest to its egress does not come up: boot
    /// refuses naming `nftables`, as it names each other mechanism a host
    /// lacks, and admits the host that has them all.
    #[test]
    fn test_firecracker_refuses_a_host_whose_packet_filter_cannot_hold_egress() {
        let lacking = [
            (
                "nftables",
                HostProbe {
                    egress: false,
                    ..host()
                },
            ),
            (
                "erofs",
                HostProbe {
                    toolbox_filesystem: false,
                    ..host()
                },
            ),
            (
                "io",
                HostProbe {
                    cgroup_controllers: vec!["cpu".into(), "memory".into(), "pids".into()],
                    ..host()
                },
            ),
        ];

        for (missing, host) in lacking {
            let refused = admitted(&host).unwrap_err();
            assert_eq!(refused.missing_mechanism(), Some(missing));
        }
        admitted(&host()).unwrap();
    }
}
