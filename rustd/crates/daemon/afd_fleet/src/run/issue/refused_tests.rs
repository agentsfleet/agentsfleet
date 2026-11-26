use super::{Census, Refused};

/// A census a refused insert read: three live runs on four workers, one hold.
const COUNTED: Census = Census {
    occupied: 3,
    held: 1,
    worker_count: 4,
};

/// Another pass can run the turn only when this one lost for room, time or a
/// moved choice; a refusal of the host or of the work would meet it again.
#[test]
fn a_refusal_rerings_only_when_another_pass_can_run_the_turn() {
    let cases = [
        (Refused::CensusFull(COUNTED), true),
        (Refused::Fenced(COUNTED), true),
        (Refused::ChoiceStale, true),
        (Refused::Scope, false),
        (Refused::HostUnproven, false),
        (Refused::HostClosed, false),
        (Refused::Placement, false),
        (Refused::Task, false),
        (Refused::Profile, false),
    ];

    for (refused, rerings) in cases {
        assert_eq!(refused.rerings(), rerings, "{}", refused.as_str());
    }
    assert_eq!(Refused::Fenced(COUNTED).census(), Some(COUNTED));
    assert_eq!(Refused::Placement.census(), None);
}
