use super::{Census, Refused};

/// A census a refused insert read: three live runs on four workers, one hold.
const COUNTED: Census = Census {
    occupied: 3,
    held: 1,
    worker_count: 4,
};

/// Another pass can run the turn only when this one lost for room, time or a
/// moved choice; a refusal of the host or of the work would meet it again.
/// Each refusal logs under its own reason.
#[test]
fn a_refusal_rerings_only_when_another_pass_can_run_the_turn() {
    let cases = [
        (Refused::CensusFull(COUNTED), true, "census_full"),
        (Refused::Fenced(COUNTED), true, "fenced"),
        (Refused::ChoiceStale, true, "choice_stale"),
        (Refused::Scope, false, "scope"),
        (Refused::HostUnproven, false, "host_unproven"),
        (Refused::HostClosed, false, "host_closed"),
        (Refused::Placement, false, "placement"),
        (Refused::Task, false, "task"),
        (Refused::Profile, false, "profile"),
    ];

    for (refused, rerings, reason) in cases {
        assert_eq!((refused.rerings(), refused.as_str()), (rerings, reason));
    }
    assert_eq!(Refused::Fenced(COUNTED).census(), Some(COUNTED));
    assert_eq!(Refused::Placement.census(), None);
}
