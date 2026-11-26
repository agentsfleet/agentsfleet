//! What this host's kernel can enforce, as the capability report states it.
//!
//! The facts alone: each building block probes its own mechanism, and each
//! engine's boot admission assembles them into one [`HostProbe`], which the
//! supervisor's heartbeat reads.

use af_wire::runner::Kvm;

/// The mechanism names a refusal and the capability report share.
pub const MECHANISM_LANDLOCK: &str = "landlock";
/// See [`MECHANISM_LANDLOCK`].
pub const MECHANISM_SECCOMP: &str = "seccomp";
/// See [`MECHANISM_LANDLOCK`].
pub const MECHANISM_BUBBLEWRAP: &str = "bubblewrap";
/// See [`MECHANISM_LANDLOCK`]; the toolbox's file system is EROFS.
pub const MECHANISM_TOOLBOX_FILESYSTEM: &str = "erofs";
/// The cgroup controllers every run's limits are written through.
///
/// `io` is among them: a host that cannot cap a run's disk throughput cannot
/// keep one run from starving the others, so it builds no sandbox.
pub const REQUIRED_CONTROLLERS: [&str; 4] = ["cpu", "io", "memory", "pids"];

/// One probe of the host, taken at boot and refreshed per heartbeat.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separately reported mechanism, as on the wire's capability report"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProbe {
    /// Landlock is enabled in the running kernel, at the interface every
    /// sandbox's ruleset needs.
    pub landlock: bool,
    /// Seccomp filtering is available.
    pub seccomp: bool,
    /// Controllers enabled in the delegated cgroup's subtree.
    pub cgroup_controllers: Vec<String>,
    /// The bubblewrap launcher is installed and runs.
    pub bubblewrap: bool,
    /// What `/dev/kvm` allows.
    pub kvm: Kvm,
    /// The kernel can mount the toolbox's file system (EROFS); without it no
    /// sandbox can be built.
    pub toolbox_filesystem: bool,
    /// The state file system takes direct I/O, so a workspace disk is cached
    /// once rather than twice; `None` when no state directory was named.
    pub workspace_direct_io: Option<bool>,
    /// The kernel built and removed an egress scope — a veth pair and its
    /// host-side `nf_tables` table — with forwarding on, so an
    /// `allow_list_egress` sandbox reaches its allowlist and nothing else.
    pub egress: bool,
}

impl HostProbe {
    /// Whether every controller a run's limits need is delegated here.
    #[must_use]
    pub fn has_required_controllers(&self) -> bool {
        self.missing_controller().is_none()
    }

    /// The first required controller this host does not delegate.
    #[must_use]
    pub fn missing_controller(&self) -> Option<&'static str> {
        REQUIRED_CONTROLLERS
            .into_iter()
            .find(|wanted| !self.cgroup_controllers.iter().any(|have| have == wanted))
    }
}
