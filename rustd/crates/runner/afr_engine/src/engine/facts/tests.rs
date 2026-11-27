#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::runner::{Isolation, SandboxEngine};

use super::{Cap, Capabilities, ParkMode, ResourceFacts};
use crate::engine::Limits;

const ASKED: Limits = Limits {
    memory_bytes: 2 << 30,
    cpu_millis: 2_000,
    pids: 512,
    disk_bytes: 4 << 30,
};

/// An engine with no process cap, asked for 512 processes, refuses naming
/// `pids`, and nothing is granted.
#[test]
fn test_admit_refuses_a_limit_the_engine_cannot_enforce() {
    let no_pids = ResourceFacts {
        pids: Cap::Absent,
        ..ResourceFacts::ENFORCED
    };

    let refused = no_pids
        .grant(SandboxEngine::Firecracker, ASKED)
        .expect_err("a cap with no mechanism is refused");

    assert_eq!(refused.detail(), "the firecracker engine cannot limit pids");
}

/// An engine that enforces every cap grants exactly what it was asked.
#[test]
fn test_a_grant_is_never_less_than_the_ask() {
    let enforced = ResourceFacts::ENFORCED
        .grant(SandboxEngine::Firecracker, ASKED)
        .expect("an engine enforcing every cap grants the ask");

    assert_eq!(*enforced.limits(), ASKED);
    assert_eq!(enforced.facts(), ResourceFacts::ENFORCED);
}

/// A park mode outside an engine's table is refused naming `park`; one in it
/// passes.
#[test]
fn test_a_park_mode_outside_the_table_is_unsupported() {
    let stop_only = Capabilities {
        engine: SandboxEngine::Firecracker,
        isolation: Isolation::HardwareVm,
        parks: &[ParkMode::Stop],
        snapshots: false,
        resources: ResourceFacts::ENFORCED,
    };

    let refused = stop_only
        .require_park(ParkMode::Pause)
        .expect_err("pause is not in the table");

    assert_eq!(refused.detail(), "the firecracker engine cannot park");
    stop_only
        .require_park(ParkMode::Stop)
        .expect("stop is in the table");
}
