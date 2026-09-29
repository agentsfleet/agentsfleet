//! Which stored answers wake the fleet again when a resolve finds the gate
//! already answered.

use afd_wire::approval::status;

use super::stood::leaves_delivery_parked;

/// A run the answer ended is parked, and so is a gate that held none.
#[test]
fn an_ended_run_or_a_runless_gate_leaves_its_delivery_parked() {
    let run = Some("019feca5-bc9b-72e8-b71f-e2714f6b0122");
    assert!(leaves_delivery_parked(status::DENIED, run));
    assert!(leaves_delivery_parked(status::TIMED_OUT, run));
    assert!(leaves_delivery_parked(status::APPROVED, None));
    assert!(leaves_delivery_parked(status::DENIED, None));
}

/// An approved run continues through the event its continuation landed, so
/// nothing is parked for a wake to reach.
#[test]
fn an_approved_run_leaves_nothing_parked() {
    let run = Some("019feca5-bc9b-72e8-b71f-e2714f6b0122");
    assert!(!leaves_delivery_parked(status::APPROVED, run));
    assert!(!leaves_delivery_parked(status::PENDING, run));
}
