use super::{Ending, SHELL_KILLED_EXIT, SIGKILL};

#[test]
fn should_report_a_signal_as_a_shell_does_and_no_status_without_one() {
    assert_eq!(Ending::Exited(3).exit_code(), Some(3));
    assert_eq!(Ending::Signaled(9).exit_code(), Some(137));
    assert_eq!(
        Ending::Signaled(i32::MAX).exit_code(),
        Some(i32::MAX),
        "saturates, never wraps"
    );
    assert_eq!(
        Ending::OutOfMemory.exit_code(),
        Some(137),
        "as the SIGKILL it was"
    );
    assert_eq!(Ending::TimedOut.exit_code(), None);
    assert_eq!(Ending::Interrupted.exit_code(), None);
}

/// The status a caller encodes for a `SIGKILL` is the one the executor
/// decodes from a shell whose child was killed, so an out-of-memory kill
/// spelled one way is read back the same way.
#[test]
fn should_encode_a_kill_as_the_exit_the_executor_decodes() {
    assert_eq!(SIGKILL, 9, "SIGKILL is 9 on every Unix");
    assert_eq!(
        Ending::Signaled(SIGKILL).exit_code(),
        Some(SHELL_KILLED_EXIT)
    );
    assert_eq!(Ending::OutOfMemory.exit_code(), Some(SHELL_KILLED_EXIT));
}
