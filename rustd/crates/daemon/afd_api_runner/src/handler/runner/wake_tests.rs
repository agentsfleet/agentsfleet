//! How a held beat's wait becomes its answer, its count and its log line.

use std::cell::Cell;

use af_core::limits::HELD_BEATS_MAX;
use af_core::test_util::trace::Capture;
use afd_fleet::lease::Waited;

use super::{EVENT_WAKE_REFUSED, woken};

const RUNNER: &str = "01924f4e-0000-7000-8000-0000000000aa";

/// What one beat answered, and whether it waited at all.
#[derive(Debug, PartialEq, Eq)]
struct Answered {
    woken: bool,
    waited: bool,
}

/// Answers a beat that asked or not, whose wait ends `waited`.
async fn answer(asked: bool, waited: Waited) -> Answered {
    let called = Cell::new(false);
    let woken = woken(asked, RUNNER, || {
        called.set(true);
        std::future::ready(waited)
    })
    .await;
    Answered {
        woken,
        waited: called.get(),
    }
}

/// Only a ring wakes; a deadline and the cap answer `false`; a beat that did
/// not ask never waits.
#[tokio::test]
async fn test_held_beat_answers_by_how_its_wait_ended() {
    let answers = [
        answer(true, Waited::Rung).await,
        answer(true, Waited::Deadline).await,
        answer(true, Waited::Capped).await,
        answer(false, Waited::Rung).await,
    ];
    assert_eq!(
        answers,
        [
            Answered {
                woken: true,
                waited: true
            },
            Answered {
                woken: false,
                waited: true
            },
            Answered {
                woken: false,
                waited: true
            },
            Answered {
                woken: false,
                waited: false
            },
        ]
    );
}

/// A beat past the cap is logged once with the cap and the runner, and a
/// held beat that ended any other way logs nothing.
#[tokio::test]
async fn test_held_beats_are_capped() {
    let log = Capture::install();
    answer(true, Waited::Rung).await;
    answer(true, Waited::Deadline).await;
    answer(true, Waited::Capped).await;

    let refused = log.only(EVENT_WAKE_REFUSED);
    let cap = HELD_BEATS_MAX.to_string();
    assert_eq!(
        (refused.field("cap"), refused.field("runner_id")),
        (Some(cap.as_str()), Some(RUNNER)),
        "{refused:?}"
    );
}

/// Each held beat is counted under how it ended, at the call that answers it.
#[cfg(feature = "test-util")]
#[tokio::test]
async fn test_held_beats_are_counted_by_how_they_ended() {
    const FAMILY: &str = "agentsfleet_runner_wake_answers_total";
    const REASONS: [&str; 3] = ["ring", "deadline", "capped"];
    let metrics = af_observability::test_util::Capture::install();
    let read = || REASONS.map(|reason| metrics.sum(FAMILY, &[("reason", reason)]));
    let before = read();

    answer(true, Waited::Rung).await;
    answer(true, Waited::Deadline).await;
    answer(true, Waited::Capped).await;
    answer(false, Waited::Rung).await;

    let after = read();
    let counted: Vec<u64> = after
        .iter()
        .zip(before)
        .map(|(now, then)| now - then)
        .collect();
    assert_eq!(
        counted,
        vec![1, 1, 1],
        "a beat that did not ask is not counted"
    );
}
