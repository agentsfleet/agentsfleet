use afd_core::error_code;

use super::{EXIT_FAILED, EXIT_TOKEN_REFUSED, exit_status};

/// A refused token ends `run` with the one status the unit will not restart
/// on; every other failure keeps the status systemd restarts after.
#[test]
fn test_only_a_refused_token_exits_with_the_no_restart_status() {
    assert_eq!(
        exit_status(error_code::RUN_INVALID_RUNNER_TOKEN),
        EXIT_TOKEN_REFUSED
    );
    assert_eq!(
        exit_status(error_code::INTERNAL_OPERATION_FAILED),
        EXIT_FAILED
    );
    assert_eq!(exit_status(error_code::RUN_LEASE_LOST), EXIT_FAILED);
}
