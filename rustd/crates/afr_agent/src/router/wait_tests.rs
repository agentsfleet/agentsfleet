//! A sandbox-side call made while the lease's build runs waits for it, then
//! runs in that sandbox or answers why there is none; a supervisor-side call
//! never waits, and a call after the build records no wait.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use afr_executor::{ExecutorSlot, ExecutorSource as _, Unavailable};
use afr_telemetry::labels::SandboxWait;
use afr_telemetry::testing::{Recorded, Tally, scoped};
use afr_tools::catalog::{FILE_READ, UPDATE_PLAN};
use afr_tools::{Lease, ToolErrorCode};

use super::Router;
use super::tests::{Counting, catalog};

/// How long the lease's build takes in each test.
const BUILD_TAKES: Duration = Duration::from_millis(40);

/// The sandbox waits the recorder saw, in order.
fn waits(recorded: &Receiver<Recorded>) -> Vec<SandboxWait> {
    recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            Recorded::SandboxWait(outcome, _elapsed) => Some(outcome),
            _other => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_sandbox_call_mid_build_waits_then_runs_in_that_sandbox() {
    let catalog = catalog();
    let selection = catalog.select(&[FILE_READ.name()]).unwrap();
    let executor = Counting::default();
    let slot = ExecutorSlot::pending();
    let router = Router::new(&selection, &slot);
    let (tally, recorded) = Tally::new();
    let (arguments, lease) = (serde_json::json!({}), Lease::default());
    let call = router.dispatch(FILE_READ.name(), &arguments, &lease);
    let build = async {
        tokio::time::sleep(BUILD_TAKES).await;
        slot.fill(Ok(&executor))
    };

    let (output, filled) = scoped(tally, async { tokio::join!(call, build) }).await;

    assert!(filled, "the build's fill is the first");
    assert_eq!(output.error_code, None, "{}", output.text);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "the call crossed into the build's sandbox"
    );
    assert_eq!(slot.held(), Some(BUILD_TAKES));
    assert_eq!(waits(&recorded), [SandboxWait::Ready]);
}

#[tokio::test(start_paused = true)]
async fn a_sandbox_call_mid_build_answers_the_failures_class() {
    for class in [Unavailable::Prepare, Unavailable::Checkout] {
        let catalog = catalog();
        let selection = catalog.select(&[FILE_READ.name()]).unwrap();
        let slot = ExecutorSlot::pending();
        let router = Router::new(&selection, &slot);
        let (tally, recorded) = Tally::new();
        let (arguments, lease) = (serde_json::json!({}), Lease::default());
        let call = router.dispatch(FILE_READ.name(), &arguments, &lease);
        let build = async {
            tokio::time::sleep(BUILD_TAKES).await;
            slot.fill(Err(class))
        };

        let (output, _filled) = scoped(tally, async { tokio::join!(call, build) }).await;

        assert_eq!(output.error_code, Some(ToolErrorCode::SandboxUnavailable));
        assert!(
            output.text.contains(FILE_READ.name()) && output.text.ends_with(class.reason()),
            "{class:?} names its class: {}",
            output.text
        );
        assert_eq!(waits(&recorded), [SandboxWait::Unavailable]);
    }
}

#[tokio::test(start_paused = true)]
async fn a_supervisor_call_never_waits_on_the_build() {
    let catalog = catalog();
    let selection = catalog.select(&[UPDATE_PLAN.name()]).unwrap();
    let slot = ExecutorSlot::pending();
    let router = Router::new(&selection, &slot);

    let (arguments, lease) = (serde_json::json!({}), Lease::default());

    let call = router.dispatch(UPDATE_PLAN.name(), &arguments, &lease);
    let output = tokio::time::timeout(BUILD_TAKES, call).await;

    assert!(
        output.is_ok_and(|output| output.error_code.is_none()),
        "the supervisor ran it while the build had not finished"
    );
    assert!(!slot.is_filled());
    assert_eq!(slot.held(), None, "nothing waited on the build");
}

#[tokio::test]
async fn a_sandbox_call_after_the_build_records_no_wait() {
    let catalog = catalog();
    let selection = catalog.select(&[FILE_READ.name()]).unwrap();
    let executor = Counting::default();
    let slot = ExecutorSlot::ready(&executor);
    let router = Router::new(&selection, &slot);
    let (tally, recorded) = Tally::new();

    let (arguments, lease) = (serde_json::json!({}), Lease::default());

    let call = router.dispatch(FILE_READ.name(), &arguments, &lease);
    let output = scoped(tally, call).await;

    assert_eq!(output.error_code, None, "{}", output.text);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        waits(&recorded).is_empty(),
        "a filled slot holds no call up"
    );
}
