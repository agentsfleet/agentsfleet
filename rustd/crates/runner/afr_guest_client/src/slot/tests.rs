//! The slot: filled once, read at once when filled, waited on when not, and
//! never filled twice however many callers race to fill it.
#![expect(
    clippy::expect_used,
    reason = "a test fails loudly on a waiter nobody woke"
)]

use std::sync::Barrier;
use std::time::Duration;

use bytes::Bytes;
use futures_util::FutureExt as _;
use futures_util::future::join_all;

use super::{AgentSlot, AgentSource as _, Outcome, Unavailable};
use crate::{GuestAgent, Process, ProcessId, Spawn};
use af_guest_wire::error::Result;

/// A guest agent that is only ever compared by address.
#[derive(Debug)]
struct Idle;

#[async_trait::async_trait]
impl GuestAgent for Idle {
    async fn spawn(&self, _spawn: &Spawn) -> Result<Process> {
        Err(af_guest_wire::error::unknown_process())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> Result<()> {
        Err(af_guest_wire::error::unknown_process())
    }

    async fn kill(&self, _process: ProcessId) -> Result<()> {
        Err(af_guest_wire::error::unknown_process())
    }
}

/// Every class a slot can be filled without a guest agent for.
const CLASSES: [Unavailable; 5] = [
    Unavailable::NoSandbox,
    Unavailable::Prepare,
    Unavailable::Checkout,
    Unavailable::Stopped,
    Unavailable::Lost,
];
/// How long the build takes in the waiting tests.
const BUILD_TAKES: Duration = Duration::from_millis(40);
/// How many callers race in the contention tests.
const CALLERS: usize = 100;
/// How long a waiting test lets its waiters run. On a paused clock it ends
/// the moment the runtime goes idle, so a waiter nobody woke fails the test
/// rather than hanging it.
const PATIENCE: Duration = Duration::from_secs(1);

/// `future`, failing the test if it is still pending after [`PATIENCE`].
async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(PATIENCE, future)
        .await
        .expect("still pending after its patience: a waiter was never woken")
}

/// Whether `outcome` holds exactly `agent`, compared by address.
fn holds(outcome: Outcome<'_>, agent: &Idle) -> bool {
    outcome.is_ok_and(|held| std::ptr::addr_eq(std::ptr::from_ref(held), std::ptr::from_ref(agent)))
}

#[tokio::test]
async fn a_ready_slot_answers_at_once_and_held_nobody_up() {
    let agent = Idle;
    let slot = AgentSlot::ready(&agent);

    assert!(
        slot.get()
            .now_or_never()
            .is_some_and(|outcome| holds(outcome, &agent)),
        "a ready slot answers at once"
    );
    assert!(slot.peek().is_some());
    assert!(slot.is_filled());
    assert_eq!(slot.held(), None, "no call found it empty");
}

#[tokio::test]
async fn an_unavailable_slot_answers_its_class_and_lends_nothing() {
    for class in CLASSES {
        let slot = AgentSlot::unavailable(class);

        assert_eq!(
            slot.get().now_or_never().map(Result::err),
            Some(Some(class)),
            "{class:?} answers at once"
        );
        assert!(slot.peek().is_none(), "{class:?} lends no guest agent");
        assert!(slot.is_filled());
    }
}

#[tokio::test]
async fn an_empty_slot_lends_nothing_without_waiting() {
    let slot = AgentSlot::pending();

    assert!(slot.peek().is_none());
    assert!(!slot.is_filled());
    assert_eq!(slot.held(), None);
}

#[tokio::test(start_paused = true)]
async fn a_call_on_an_empty_slot_waits_for_the_fill_and_its_wait_is_held() {
    let agent = Idle;
    let slot = AgentSlot::pending();
    let build = async {
        tokio::time::sleep(BUILD_TAKES).await;
        slot.fill(Ok(&agent))
    };

    let (outcome, filled) = bounded(async { tokio::join!(slot.get(), build) }).await;

    assert!(filled, "the build's fill is the first");
    assert!(holds(outcome, &agent));
    assert_eq!(slot.held(), Some(BUILD_TAKES));
}

#[tokio::test(start_paused = true)]
async fn a_failed_build_answers_every_waiter_with_its_class() {
    let slot = AgentSlot::pending();
    let waiters = join_all((0..CALLERS).map(|_| slot.get()));
    let build = async {
        tokio::time::sleep(BUILD_TAKES).await;
        slot.fill(Err(Unavailable::Checkout))
    };

    let (outcomes, filled) = bounded(async { tokio::join!(waiters, build) }).await;

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
    let agent = Idle;
    let slot = AgentSlot::pending();

    assert!(slot.fill(Ok(&agent)));
    assert!(
        !slot.fill(Err(Unavailable::Prepare)),
        "the first fill stands"
    );

    assert!(
        slot.get()
            .now_or_never()
            .is_some_and(|outcome| holds(outcome, &agent)),
        "the first fill answers at once"
    );
}

#[test]
fn of_many_callers_racing_to_fill_exactly_one_does() {
    let slot = AgentSlot::pending();
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
            .filter_map(|racer| {
                racer
                    .join()
                    .unwrap_or_else(|panicked| std::panic::resume_unwind(panicked))
            })
            .collect()
    });

    assert_eq!(won.len(), 1, "one fill wins: {won:?}");
    let filled = slot.get().now_or_never();
    assert_eq!(filled.and_then(Result::err), won.first().copied());
}

#[test]
fn each_class_names_itself_and_reads_as_the_end_of_a_tool_sentence() {
    let names: Vec<&str> = CLASSES.iter().map(|class| class.as_str()).collect();
    assert_eq!(
        names,
        ["no_sandbox", "prepare", "checkout", "stopped", "lost"]
    );
    for class in CLASSES {
        let reason = class.reason();
        assert!(reason.starts_with("runs in the sandbox, and "), "{reason}");
        assert!(!reason.contains('/'), "a reason names no path: {reason}");
    }
}
