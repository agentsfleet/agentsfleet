//! The kill a process's events queue when their reader leaves before its end.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a queue it cannot fill"
)]

use std::sync::Arc;

use afd_core::test_util::trace::Capture;
use tokio::sync::mpsc;

use super::{CallIds, EVENT_KILL_UNQUEUED, kill, kill_on_abandon};
use crate::api::ProcessId;

/// The process whose reader left.
const PROCESS: u64 = 7;

#[test]
fn a_hook_whose_client_is_gone_sends_nothing() {
    let (calls, mut queue) = mpsc::channel(1);
    let hook = kill_on_abandon(
        calls.downgrade(),
        Arc::new(CallIds::default()),
        ProcessId::new(PROCESS),
    );
    drop(calls);

    hook();

    assert!(
        queue.try_recv().is_err(),
        "closing the connection ended the process; no kill is queued"
    );
}

#[test]
fn a_kill_a_full_backlog_will_not_take_is_logged_and_lost() {
    let capture = Capture::install();
    let ids = Arc::new(CallIds::default());
    let (calls, mut queue) = mpsc::channel(1);
    calls
        .try_send(kill(&ids, ProcessId::new(PROCESS)).unwrap())
        .unwrap();
    let hook = kill_on_abandon(calls.downgrade(), Arc::clone(&ids), ProcessId::new(PROCESS));

    hook();

    let logged = capture.only(EVENT_KILL_UNQUEUED);
    assert_eq!(
        logged.field("process_id"),
        Some(PROCESS.to_string().as_str())
    );
    assert!(logged.field("error_code").is_some(), "logged under a code");
    queue.try_recv().unwrap();
    assert!(
        queue.try_recv().is_err(),
        "only the call that filled the backlog is queued"
    );
}
