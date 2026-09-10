//! What one page of the inbox contains, in what order, and how the next one
//! resumes.
//!
//! # Why these are proven against a live datastore and not a stub
//!
//! All three are properties of `SELECT_GATE_PAGE` itself — a filter predicate,
//! an `ORDER BY`, and a row-wise tuple comparison. A stub would assert that the
//! code passes an argument, which was never in doubt; what is under test is
//! what Postgres does with it.
//!
//! # Split from `integration_inbox.rs`
//!
//! That suite is the resolve race, the workspace scope and the sweep. Reading a
//! page is a separate concern and, at RULE FLL's cap, a separate file.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_approval::{Cursor, Decision, Filter, GateStatus, Resolution};

use crate::lane::{Lane, NOW_MS, WINDOW_MS};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// A page big enough that nothing under test is lost to the limit.
const WHOLE_PAGE: i64 = 50;

/// The gap between two seeded instants, wide enough that the order is the
/// statement's and never the clock's.
const A_MOMENT_MS: i64 = 1_000;

/// The status filter is a filter: absent narrows nothing.
///
/// It used to mean `pending`, which made it the one filter here that could not
/// be turned off — an inbox wanting every state had to ask once per state, and
/// the dashboard did exactly that.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_absent_status_reads_every_state_and_a_named_one_narrows() {
    let lane = Lane::isolated().await;
    let answered = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let resolved = lane
        .inbox
        .resolve(
            &answered,
            Decision::Approved,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");
    assert!(matches!(resolved, Resolution::Resolved(_)));

    let every = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");
    assert_eq!(
        every.len(),
        2,
        "an absent status reads the answered gate as well as the waiting one"
    );

    let waiting = lane
        .inbox
        .page(
            &lane.workspace,
            Filter {
                status: Some(GateStatus::Pending),
                ..Filter::default()
            },
            None,
            WHOLE_PAGE,
        )
        .await
        .expect("the queue read must not fault");
    assert_eq!(waiting.len(), 1, "a named status still narrows to it");
    assert_eq!(
        waiting
            .first()
            .expect("the one row the assertion above counted")
            .status,
        GateStatus::Pending.as_str(),
    );

    // And a state nothing is in reads empty rather than falling back to pending.
    let denied = lane
        .inbox
        .page(
            &lane.workspace,
            Filter {
                status: Some(GateStatus::Denied),
                ..Filter::default()
            },
            None,
            WHOLE_PAGE,
        )
        .await
        .expect("the queue read must not fault");
    assert!(denied.is_empty(), "no gate was denied");
}

/// The page reads newest first.
///
/// It read oldest-first while `status` was mandatory, because the pending
/// queue's oldest row is its most urgent. Now that an absent status returns all
/// five states, `LIMIT` picks from the whole history, and oldest-first would
/// make page one the most ANCIENT gates in the workspace — settled rows, with
/// today's queue off the page entirely.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_page_reads_newest_first() {
    let lane = Lane::isolated().await;
    // Seeded oldest-first, so a page that merely echoed insertion order would
    // come back in exactly the wrong sequence and the assertion would catch it.
    let oldest = lane
        .seed_gate_at(NOW_MS - 2 * A_MOMENT_MS, NOW_MS + WINDOW_MS)
        .await;
    let middle = lane
        .seed_gate_at(NOW_MS - A_MOMENT_MS, NOW_MS + WINDOW_MS)
        .await;
    let newest = lane.seed_gate_at(NOW_MS, NOW_MS + WINDOW_MS).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, WHOLE_PAGE)
        .await
        .expect("the queue read must not fault");

    let order: Vec<&str> = page.iter().map(|row| row.action_id.as_str()).collect();
    assert_eq!(
        order,
        vec![newest.as_str(), middle.as_str(), oldest.as_str()],
        "newest first, which is what the inbox renders top-down"
    );
}

/// The cursor walks every gate raised in the same millisecond.
///
/// One run parks several tools at once, so siblings sharing an instant are
/// ordinary rather than exotic. The predicate is a row-wise
/// `(created_at, id) < ($6, $7::uuid)` for exactly this: an instant alone would
/// resume PAST every sibling of the cursor row and lose them silently.
///
/// It also proves the uuid cast. The comparison spelled `g.id::text` until this
/// change; a page walked with the wrong form either faults or skips, and both
/// show up here as a row count that is not the number seeded.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_cursor_walks_every_gate_raised_in_one_millisecond() {
    const SIBLINGS: usize = 5;
    /// Small enough that the walk takes several pages over `SIBLINGS` rows.
    const PAGE: i64 = 2;

    let lane = Lane::isolated().await;
    let mut seeded = Vec::with_capacity(SIBLINGS);
    for _ in 0..SIBLINGS {
        // The same instant every time: the tie is the whole point.
        seeded.push(lane.seed_gate_at(NOW_MS, NOW_MS + WINDOW_MS).await);
    }

    let mut walked: Vec<String> = Vec::with_capacity(SIBLINGS);
    let mut resume: Option<(i64, String)> = None;
    loop {
        let cursor = resume.as_ref().map(|(at, id)| Cursor {
            created_at: *at,
            gate_id: id.as_str(),
        });
        let page = lane
            .inbox
            .page(&lane.workspace, Filter::default(), cursor, PAGE)
            .await
            .expect("the queue read must not fault");
        let Some(last) = page.last() else { break };
        resume = Some((last.created_at, last.gate_id.clone()));
        walked.extend(page.iter().map(|row| row.action_id.clone()));
        if i64::try_from(page.len()).is_ok_and(|read| read < PAGE) {
            break;
        }
    }

    walked.sort_unstable();
    seeded.sort_unstable();
    assert_eq!(
        walked, seeded,
        "every sibling comes back exactly once across the walk"
    );
}
