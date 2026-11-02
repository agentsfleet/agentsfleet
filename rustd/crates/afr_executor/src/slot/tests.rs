//! The slot: filled once, read at once when filled, waited on when not, and
//! never filled twice however many callers race to fill it.

use std::sync::Barrier;
use std::time::Duration;

use bytes::Bytes;
use futures_util::future::join_all;

use super::{ExecutorSlot, ExecutorSource as _, Outcome, Unavailable};
use crate::error::Result;
use crate::{Executor, Process, ProcessId, Spawn};

/// An executor that is only ever compared by address.
#[derive(Debug)]
struct Idle;

#[async_trait::async_trait]
impl Executor for Idle {
    async fn spawn(&self, _spawn: &Spawn) -> Result<Process> {
        Err(crate::error::unknown_process())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> Result<()> {
        Err(crate::error::unknown_process())
    }

    async fn kill(&self, _process: ProcessId) -> Result<()> {
        Err(crate::error::unknown_process())
    }
}

/// Every class a slot can be filled without an executor for.
const CLASSES: [Unavailable; 4] = [
    Unavailable::NoSandbox,
    Unavailable::Prepare,
    Unavailable::Checkout,
    Unavailable::Stopped,
];
/// How long the build takes in the waiting tests.
const BUILD_TAKES: Duration = Duration::from_millis(40);
/// How many callers race in the contention tests.
const CALLERS: usize = 100;

/// Whether `outcome` holds exactly `executor`, compared by address.
fn holds(outcome: Outcome<'_>, executor: &Idle) -> bool {
    outcome
        .is_ok_and(|held| std::ptr::addr_eq(std::ptr::from_ref(held), std::ptr::from_ref(executor)))
}

#[tokio::test]
async fn a_ready_slot_answers_at_once_and_held_nobody_up() {
    let executor = Idle;
    let slot = ExecutorSlot::ready(&executor);

    assert!(holds(slot.get().await, &executor));
    assert!(slot.peek().is_some());
    assert!(slot.is_filled());
    assert_eq!(slot.held(), None, "no call found it empty");
}

#[tokio::test]
async fn an_unavailable_slot_answers_its_class_and_lends_nothing() {
    for class in CLASSES {
        let slot = ExecutorSlot::unavailable(class);

        assert_eq!(slot.get().await.err(), Some(class));
        assert!(slot.peek().is_none(), "{class:?} lends no executor");
        assert!(slot.is_filled());
    }
}

#[tokio::test]
async fn an_empty_slot_lends_nothing_without_waiting() {
    let slot = ExecutorSlot::pending();

    assert!(slot.peek().is_none());
    assert!(!slot.is_filled());
    assert_eq!(slot.held(), None);
}

#[tokio::test(start_paused = true)]
async fn a_call_on_an_empty_slot_waits_for_the_fill_and_its_wait_is_held() {
    let executor = Idle;
    let slot = ExecutorSlot::pending();
    let build = async {
        tokio::time::sleep(BUILD_TAKES).await;
        slot.fill(Ok(&executor))
    };

    let (outcome, filled) = tokio::join!(slot.get(), build);

    assert!(filled, "the build's fill is the first");
    assert!(holds(outcome, &executor));
    assert_eq!(slot.held(), Some(BUILD_TAKES));
}

#[tokio::test(start_paused = true)]
async fn a_failed_build_answers_every_waiter_with_its_class() {
    let slot = ExecutorSlot::pending();
    let waiters = join_all((0..CALLERS).map(|_| slot.get()));
    let build = async {
        tokio::time::sleep(BUILD_TAKES).await;
        slot.fill(Err(Unavailable::Checkout))
    };

    let (outcomes, filled) = tokio::join!(waiters, build);

    assert!(filled);
    assert_eq!(outcomes.len(), CALLERS);
    assert!(
        outcomes
            .iter()
            .all(|outcome| outcome.err() == Some(Unavailable::Checkout)),
        "every waiter reads the failure"
    );
    assert_eq!(slot.held(), Some(BUILD_TAKES));
}

#[tokio::test]
async fn a_second_fill_changes_nothing() {
    let executor = Idle;
    let slot = ExecutorSlot::pending();

    assert!(slot.fill(Ok(&executor)));
    assert!(
        !slot.fill(Err(Unavailable::Prepare)),
        "the first fill stands"
    );

    assert!(holds(slot.get().await, &executor));
}

#[test]
fn of_many_callers_racing_to_fill_exactly_one_does() {
    let slot = ExecutorSlot::pending();
    let start = Barrier::new(CALLERS);

    let won: Vec<Unavailable> = std::thread::scope(|scope| {
        let racers: Vec<_> = CLASSES
            .iter()
            .copied()
            .cycle()
            .take(CALLERS)
            .map(|class| {
                let (slot, start) = (&slot, &start);
                scope.spawn(move || {
                    start.wait();
                    slot.fill(Err(class)).then_some(class)
                })
            })
            .collect();
        racers
            .into_iter()
            .filter_map(|racer| racer.join().ok().flatten())
            .collect()
    });

    assert_eq!(won.len(), 1, "one fill wins: {won:?}");
    let filled = futures_util::FutureExt::now_or_never(slot.get());
    assert_eq!(filled.and_then(Result::err), won.first().copied());
}

#[test]
fn each_class_names_itself_and_reads_as_the_end_of_a_tool_sentence() {
    let names: Vec<&str> = CLASSES.iter().map(|class| class.as_str()).collect();
    assert_eq!(names, ["no_sandbox", "prepare", "checkout", "stopped"]);
    for class in CLASSES {
        let reason = class.reason();
        assert!(reason.starts_with("runs in the sandbox, and "), "{reason}");
        assert!(!reason.contains('/'), "a reason names no path: {reason}");
    }
}
