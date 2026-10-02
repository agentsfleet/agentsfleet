//! What this host's kernel can enforce, as the capability report states it.

/// Whether `/dev/kvm` exists, and whether this process may open it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kvm {
    /// No such device: a microVM engine cannot run here.
    Absent,
    /// The device exists, but this process may not open it for reading and
    /// writing.
    Denied,
    /// The device exists and opens: a microVM engine can run here.
    Usable,
}

/// One probe of the host, taken at boot and refreshed per heartbeat.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separately reported mechanism, as on the wire's capability report"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProbe {
    /// Landlock is enabled in the running kernel.
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
}
