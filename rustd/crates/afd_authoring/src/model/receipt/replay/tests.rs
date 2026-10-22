//! The pending replay window excludes its recorded deadline.

use super::{Decision, Receipt};
use afd_core::error_code;
use jiff::Timestamp;

#[test]
fn a_pending_replay_needs_durable_uncertainty_at_the_exact_dispatch_deadline() {
    let deadline = Timestamp::constant(100, 0);
    let receipt = Receipt {
        id: String::new(),
        sequence: 1,
        request_digest: String::new(),
        status: "pending".into(),
        result: None,
        dispatch_expires_at: deadline,
    };
    let before = receipt
        .decision(Timestamp::constant(99, 999_999_999))
        .expect("valid pending state");
    let Some(Decision::Refused(error)) = before else {
        panic!("a pending call before its deadline remains in flight");
    };
    assert_eq!(error.code(), error_code::AUTHORING_STATE);
    for now in [deadline, Timestamp::constant(100, 1)] {
        assert!(
            receipt
                .decision(now)
                .expect("valid pending state")
                .is_none(),
            "at or after expiry, record uncertainty instead of waiting or replaying"
        );
    }
}
