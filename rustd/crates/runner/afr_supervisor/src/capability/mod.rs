//! What this host can enforce, as the heartbeat states it.
//!
//! The capability report names the engine and the wall it builds — the
//! daemon checks them against the host's assigned tier and degrades a host
//! whose engine is not its tier's. A self-test carries the host's mechanisms
//! as named checks: whether the kernel confines files and system calls,
//! whether the cgroup controllers are delegated, whether `/dev/kvm` lets a
//! microVM engine run here, whether its packet filter can hold a guest to
//! its egress, and whether the kernel can mount the toolbox, without which
//! no sandbox builds.

use std::borrow::Cow;

use af_core::spelling::to_spelling;
use af_wire::runner::{
    CapabilityReport, NetworkPolicy, SandboxTier, SelftestCheck, SelftestReport,
};
use af_wire::runner::{Isolation, Kvm};
use afr_engine::{
    Capabilities, HostProbe, MECHANISM_LANDLOCK, MECHANISM_NFTABLES, MECHANISM_SECCOMP,
};
use serde::Serialize;

const CHECK_CGROUP: &str = "cgroup_controllers";
const CHECK_KVM: &str = "kvm";
const CHECK_TOOLBOX: &str = "toolbox_filesystem";
const CHECK_DIRECT_IO: &str = "workspace_direct_io";

const LANDLOCK_ON: &str = "Landlock is enabled, so file system access can be confined.";
const LANDLOCK_OFF: &str =
    "Landlock is not enabled in this kernel; file system access cannot be confined.";
const SECCOMP_ON: &str = "Seccomp is available, so system calls can be filtered.";
const SECCOMP_OFF: &str = "Seccomp is not available; system calls cannot be filtered.";
/// Each names every controller in `afr_engine::REQUIRED_CONTROLLERS`, which a
/// test holds them to.
const CGROUP_ON: &str = "The cpu, io, memory and pids controllers are delegated to this runner.";
const CGROUP_OFF: &str = "The cpu, io, memory or pids controller is not delegated to this runner.";
const KVM_USABLE: &str = "The KVM device opens, so a microVM engine can run here.";
const KVM_DENIED: &str = "The KVM device exists but this runner cannot open it.";
const KVM_ABSENT: &str = "There is no KVM device, so no microVM engine can run here.";
const NFTABLES_ON: &str =
    "The kernel built and removed an nftables egress table with forwarding on, so egress is held.";
const NFTABLES_OFF: &str = "The kernel would not build an nftables egress table, or does not \
     forward IPv4, so no microVM engine starts here.";
const TOOLBOX_ON: &str = "The kernel can mount the toolbox's EROFS image.";
const TOOLBOX_OFF: &str = "The kernel cannot mount EROFS, so no sandbox can be built here.";
const DIRECT_IO_ON: &str =
    "The state file system takes direct I/O, so a workspace disk is cached once.";
const DIRECT_IO_OFF: &str = "The state file system refuses direct I/O, so a workspace disk falls \
     back to buffered and is cached twice.";

/// The capability report a heartbeat carries: the engine, its wall and what
/// `/dev/kvm` allows. `sandboxes_max` is the engine's admission bound, absent
/// when it reserves no memory.
#[must_use]
pub fn capability_report<'a>(
    probe: &HostProbe,
    engine: &Capabilities,
    sandboxes_max: Option<u32>,
) -> CapabilityReport<'a> {
    CapabilityReport {
        durable_tasks_v1: true,
        build_id: af_core::build::SOURCE_ID.map(Cow::Borrowed),
        engine: engine.engine,
        isolation: engine.isolation,
        kvm: probe.kvm,
        sandboxes_max,
    }
}

/// A self-test over the probe, labelled with the assignment it ran under.
#[must_use]
pub fn selftest<'a>(
    probe: &HostProbe,
    engine: &Capabilities,
    tier: SandboxTier,
    network: NetworkPolicy,
) -> SelftestReport<'a> {
    let checks = checks(probe, engine);
    SelftestReport {
        all_ok: checks.iter().all(|check| check.ok),
        checks,
        sandbox_tier: spelling(&tier),
        network_policy: spelling(&network),
    }
}

/// What `agentsfleet-runner probe` answers: the report a heartbeat carries,
/// and every check, which adds the facts the report has no field for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeAnswer<'a> {
    /// The mechanisms, as a heartbeat states them.
    pub capability_report: CapabilityReport<'a>,
    /// Each fact as a named check with its prose.
    pub checks: Vec<SelftestCheck<'a>>,
}

/// The probe command's answer for this host. No engine is built to probe,
/// so no admission bound is stated.
#[must_use]
pub fn probe_answer<'a>(probe: &'a HostProbe, engine: &Capabilities) -> ProbeAnswer<'a> {
    ProbeAnswer {
        capability_report: capability_report(probe, engine, None),
        checks: checks(probe, engine),
    }
}

/// Every fact the probe found that `engine` relies on, as a named check: the
/// KVM device and the packet filter only for an engine behind a virtual
/// machine, so a host is never judged on what its engine does not use.
/// Direct I/O only when the probe was given a state directory to try it on.
fn checks<'a>(probe: &HostProbe, engine: &Capabilities) -> Vec<SelftestCheck<'a>> {
    let kvm = match probe.kvm {
        Kvm::Usable => (true, KVM_USABLE),
        Kvm::Denied => (false, KVM_DENIED),
        Kvm::Absent => (false, KVM_ABSENT),
    };
    let mut checks = vec![
        check(
            MECHANISM_LANDLOCK,
            verdict(probe.landlock, LANDLOCK_ON, LANDLOCK_OFF),
        ),
        check(
            MECHANISM_SECCOMP,
            verdict(probe.seccomp, SECCOMP_ON, SECCOMP_OFF),
        ),
    ];
    checks.push(check(
        CHECK_CGROUP,
        verdict(probe.has_required_controllers(), CGROUP_ON, CGROUP_OFF),
    ));
    if engine.isolation == Isolation::HardwareVm {
        let nftables = verdict(probe.egress, NFTABLES_ON, NFTABLES_OFF);
        checks.extend([check(CHECK_KVM, kvm), check(MECHANISM_NFTABLES, nftables)]);
    }
    checks.push(check(
        CHECK_TOOLBOX,
        verdict(probe.toolbox_filesystem, TOOLBOX_ON, TOOLBOX_OFF),
    ));
    checks.extend(
        probe
            .workspace_direct_io
            .map(|on| check(CHECK_DIRECT_IO, verdict(on, DIRECT_IO_ON, DIRECT_IO_OFF))),
    );
    checks
}

const fn verdict(ok: bool, on: &'static str, off: &'static str) -> (bool, &'static str) {
    if ok { (true, on) } else { (false, off) }
}

const fn check<'a>(name: &'static str, (ok, detail): (bool, &'static str)) -> SelftestCheck<'a> {
    SelftestCheck {
        name: Cow::Borrowed(name),
        ok,
        detail: Cow::Borrowed(detail),
    }
}

/// A fieldless wire enum's own spelling, as its serde declaration writes it.
fn spelling<'a, T: Serialize>(value: &T) -> Cow<'a, str> {
    to_spelling(value).map_or(Cow::Borrowed(""), Cow::Owned)
}

mod host;
pub(crate) use self::host::HostReader;

#[cfg(test)]
mod tests;
