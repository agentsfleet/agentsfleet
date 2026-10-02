//! What this host can enforce, as the heartbeat states it.
//!
//! The capability report carries the wire's mechanisms — the daemon checks
//! them against the host's assigned tier and degrades a host that falls short.
//! A self-test carries those same facts as named checks, plus two the report
//! has no field for: whether `/dev/kvm` lets a microVM engine run here, and
//! whether the kernel can mount the toolbox, without which no sandbox builds.

use std::borrow::Cow;

use afd_wire::runner::{
    CapabilityReport, NetworkPolicy, SandboxTier, SelftestCheck, SelftestReport,
};
use afr_sandbox::{
    HostProbe, Kvm, MECHANISM_BUBBLEWRAP, MECHANISM_LANDLOCK, MECHANISM_SECCOMP,
    REQUIRED_CONTROLLERS,
};
use serde::Serialize;

const CHECK_CGROUP: &str = "cgroup_controllers";
const CHECK_KVM: &str = "kvm";
const CHECK_TOOLBOX: &str = "toolbox_filesystem";

const LANDLOCK_ON: &str = "Landlock is enabled, so file system access can be confined.";
const LANDLOCK_OFF: &str =
    "Landlock is not enabled in this kernel; file system access cannot be confined.";
const SECCOMP_ON: &str = "Seccomp is available, so system calls can be filtered.";
const SECCOMP_OFF: &str = "Seccomp is not available; system calls cannot be filtered.";
const BUBBLEWRAP_ON: &str = "The bubblewrap launcher is installed and runs.";
const BUBBLEWRAP_OFF: &str = "The bubblewrap launcher is missing or does not run.";
const CGROUP_ON: &str = "The cpu, memory and pids controllers are delegated to this runner.";
const CGROUP_OFF: &str = "The cpu, memory or pids controller is not delegated to this runner.";
const KVM_USABLE: &str = "The KVM device opens, so a microVM engine can run here.";
const KVM_DENIED: &str = "The KVM device exists but this runner cannot open it.";
const KVM_ABSENT: &str = "There is no KVM device, so only the bubblewrap engine can run here.";
const TOOLBOX_ON: &str = "The kernel can mount the toolbox's EROFS image.";
const TOOLBOX_OFF: &str = "The kernel cannot mount EROFS, so no sandbox can be built here.";

/// The capability report a heartbeat carries.
///
/// Egress enforcement is not offered yet: a sandbox here has no network at all.
#[must_use]
pub fn capability_report(probe: &HostProbe) -> CapabilityReport<'_> {
    CapabilityReport {
        landlock: probe.landlock,
        seccomp: probe.seccomp,
        cgroup_controllers: probe
            .cgroup_controllers
            .iter()
            .map(|controller| Cow::Borrowed(controller.as_str()))
            .collect(),
        bubblewrap: probe.bubblewrap,
        egress_enforcement: false,
    }
}

/// A self-test over the probe, labelled with the assignment it ran under.
#[must_use]
pub fn selftest<'a>(
    probe: &HostProbe,
    tier: SandboxTier,
    network: NetworkPolicy,
) -> SelftestReport<'a> {
    let checks = checks(probe);
    SelftestReport {
        all_ok: checks.iter().all(|check| check.ok),
        checks,
        sandbox_tier: spelling(tier),
        network_policy: spelling(network),
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

/// The probe command's answer for this host.
#[must_use]
pub fn probe_answer(probe: &HostProbe) -> ProbeAnswer<'_> {
    ProbeAnswer {
        capability_report: capability_report(probe),
        checks: checks(probe),
    }
}

/// Every fact the probe found, as a named check.
fn checks<'a>(probe: &HostProbe) -> Vec<SelftestCheck<'a>> {
    let kvm = match probe.kvm {
        Kvm::Usable => (true, KVM_USABLE),
        Kvm::Denied => (false, KVM_DENIED),
        Kvm::Absent => (false, KVM_ABSENT),
    };
    vec![
        check(
            MECHANISM_LANDLOCK,
            verdict(probe.landlock, LANDLOCK_ON, LANDLOCK_OFF),
        ),
        check(
            MECHANISM_SECCOMP,
            verdict(probe.seccomp, SECCOMP_ON, SECCOMP_OFF),
        ),
        check(
            MECHANISM_BUBBLEWRAP,
            verdict(probe.bubblewrap, BUBBLEWRAP_ON, BUBBLEWRAP_OFF),
        ),
        check(
            CHECK_CGROUP,
            verdict(has_required_controllers(probe), CGROUP_ON, CGROUP_OFF),
        ),
        check(CHECK_KVM, kvm),
        check(
            CHECK_TOOLBOX,
            verdict(probe.toolbox_filesystem, TOOLBOX_ON, TOOLBOX_OFF),
        ),
    ]
}

fn has_required_controllers(probe: &HostProbe) -> bool {
    REQUIRED_CONTROLLERS.iter().all(|required| {
        probe
            .cgroup_controllers
            .iter()
            .any(|found| found == required)
    })
}

const fn verdict(ok: bool, on: &'static str, off: &'static str) -> (bool, &'static str) {
    if ok { (true, on) } else { (false, off) }
}

fn check<'a>(name: &'static str, (ok, detail): (bool, &'static str)) -> SelftestCheck<'a> {
    SelftestCheck {
        name: Cow::Borrowed(name),
        ok,
        detail: Cow::Borrowed(detail),
    }
}

/// A fieldless wire enum's own spelling, as its serde declaration writes it.
fn spelling<'a, T: Serialize>(value: T) -> Cow<'a, str> {
    serde_json::to_value(value)
        .ok()
        .and_then(|spelled| spelled.as_str().map(str::to_owned))
        .map_or(Cow::Borrowed(""), Cow::Owned)
}

#[cfg(test)]
#[path = "capability/tests.rs"]
mod tests;
