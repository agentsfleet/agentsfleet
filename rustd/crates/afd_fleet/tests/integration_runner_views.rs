//! Operator runner reads against live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::requests;
use crate::support;
use crate::view_heartbeat;
use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_runner::{KeysetCursor, PageLimit, RunnerEventFilter};
use afd_wire::admin::{RunnerAdminAction, RunnerEventType};
use afd_wire::runner::{NetworkPolicy, RunnerLiveness, SandboxTier};

use self::requests::{ENROLLED_AT, enrolment};
use self::support::Fixtures;
use self::view_heartbeat::view_heartbeat;

const ACTOR: &str = "fixture:platform-operator";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runner_views_parity() {
    let fixtures = Fixtures::create().await;
    let seeded = seed_runner_views(&fixtures).await;
    assert_runner_pages(&fixtures, &seeded).await;
    assert_runner_detail(&fixtures, &seeded.live_runner).await;
    assert_event_pages(&fixtures, &seeded.live_runner).await;
    fixtures.cleanup().await;
}

struct SeededViews {
    live_runner: Uuid7,
    ordered_ids: Vec<String>,
}

async fn seed_runner_views(fixtures: &Fixtures) -> SeededViews {
    let runners = enroll_view_runners(fixtures).await;
    let live_runner = runners
        .first()
        .expect("the fixture enrolled three runners")
        .clone();
    exercise_view_runner(fixtures, &live_runner).await;
    let mut ordered_ids = runners
        .into_iter()
        .map(|runner| runner.as_str().to_owned())
        .collect::<Vec<_>>();
    ordered_ids.sort_by(|left, right| right.cmp(left));
    SeededViews {
        live_runner,
        ordered_ids,
    }
}

async fn enroll_view_runners(fixtures: &Fixtures) -> Vec<Uuid7> {
    let mut runners = Vec::new();
    for host in [
        "view-a.fixture.test",
        "view-b.fixture.test",
        "view-c.fixture.test",
    ] {
        let mut request = enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1);
        request.host_id = Cow::Borrowed(host);
        let enrolled = fixtures
            .runners()
            .register(&request, UnixMillis::from_millis(ENROLLED_AT))
            .await
            .expect("the runner enrols");
        runners.push(enrolled.runner_id);
    }
    runners
}

async fn exercise_view_runner(fixtures: &Fixtures, live_runner: &Uuid7) {
    let heartbeat = view_heartbeat();
    fixtures
        .runners()
        .heartbeat(
            live_runner,
            &heartbeat,
            UnixMillis::from_millis(ENROLLED_AT + 1),
        )
        .await
        .expect("the heartbeat lands");
    fixtures
        .runners()
        .transition(
            live_runner,
            RunnerAdminAction::Cordon,
            ACTOR,
            UnixMillis::from_millis(ENROLLED_AT + 2),
        )
        .await
        .expect("the runner is cordoned");
    let _rotated = fixtures
        .runners()
        .rotate_token(live_runner, ACTOR, UnixMillis::from_millis(ENROLLED_AT + 3))
        .await
        .expect("the token rotates");
}

/// Each page has its own database snapshot while sibling tests enrol runners.
/// Grade the seeded rows and their order; a stable global total is not promised
/// across pages, and would make this test depend on its neighbours' scheduling.
async fn assert_runner_pages(fixtures: &Fixtures, seeded: &SeededViews) {
    let limit = PageLimit::new(2).expect("two is a valid page limit");
    let now = UnixMillis::from_millis(ENROLLED_AT + 4);

    let mut cursor: Option<KeysetCursor> = None;
    let mut walked = Vec::new();
    let mut totals = Vec::new();
    loop {
        let page = fixtures
            .runners()
            .list_runners(cursor.as_ref(), limit, now)
            .await
            .expect("the page loads");
        totals.push(page.total());
        // Cloned before the page is consumed: `next_cursor` borrows from it,
        // and the walk needs the boundary to outlive the rows it came with.
        cursor = page.next_cursor().cloned();
        walked.extend(page.into_items());
        // Concurrent tests can keep enrolling runners; completing our seeded
        // set bounds this walk without depending on those unrelated writes.
        let found = walked
            .iter()
            .filter(|item| seeded.ordered_ids.iter().any(|id| id == item.id().as_str()))
            .count();
        if cursor.is_none() || found == seeded.ordered_ids.len() {
            break;
        }
    }

    let seeded_count = i64::try_from(seeded.ordered_ids.len()).expect("three fits an i64");
    assert!(
        totals.iter().all(|total| *total >= seeded_count),
        "a page reported a total that cannot account for the seeded runners: {totals:?}"
    );

    let ids = walked
        .iter()
        .map(|item| item.id().as_str().to_owned())
        .collect::<Vec<_>>();
    let unique = ids.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(
        ids.len(),
        unique.len(),
        "the composite cursor repeated a row"
    );

    let seen = ids
        .iter()
        .filter(|id| seeded.ordered_ids.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        seen, seeded.ordered_ids,
        "the composite cursor skips no ties"
    );

    for item in walked
        .iter()
        .filter(|item| seeded.ordered_ids.contains(&item.id().as_str().to_owned()))
    {
        let expected = if item.id() == &seeded.live_runner {
            RunnerLiveness::Online
        } else {
            RunnerLiveness::Registered
        };
        assert_eq!(item.liveness(), expected);
    }
}

async fn assert_runner_detail(fixtures: &Fixtures, runner: &Uuid7) {
    let detail = fixtures
        .runners()
        .runner_detail(runner, UnixMillis::from_millis(ENROLLED_AT + 4))
        .await
        .expect("the detail loads");
    assert_eq!(
        detail.item().admin_state(),
        afd_wire::admin::AdminState::Cordoned
    );
    assert_eq!(detail.item().liveness(), RunnerLiveness::Online);
    assert_eq!(detail.active_lease_count(), 0);
    assert_eq!(detail.active_fleet_count(), 0);
    assert_eq!(detail.leases_acquired(), 0);
    assert_eq!(detail.leases_succeeded(), 0);
    assert_eq!(detail.leases_failed(), 0);
    assert_eq!(detail.leases_expired(), 0);
    assert!(detail.item().assigned_policy().is_some());
    assert!(detail.item().achievable().is_some());
    let selftest = detail.selftest().expect("the stored self-test decodes");
    assert!(selftest.all_ok);
    assert_eq!(selftest.checks.len(), 1);
    assert_eq!(selftest.sandbox_tier, "dev_none");
}

async fn assert_event_pages(fixtures: &Fixtures, runner: &Uuid7) {
    let limit = PageLimit::new(2).expect("two is a valid page limit");
    let unfiltered = RunnerEventFilter::default();
    let first = fixtures
        .runners()
        .runner_events(runner, &unfiltered, None, limit)
        .await
        .expect("the first event page loads");
    let second = fixtures
        .runners()
        .runner_events(runner, &unfiltered, first.next_cursor(), limit)
        .await
        .expect("the second event page loads");
    let third = fixtures
        .runners()
        .runner_events(runner, &unfiltered, second.next_cursor(), limit)
        .await
        .expect("the terminal event page loads");
    assert_eq!((first.total(), second.total(), third.total()), (4, 4, 4));
    assert!(third.items().is_empty());
    assert!(third.next_cursor().is_none());
    let event_types = first
        .into_items()
        .into_iter()
        .chain(second.into_items())
        .map(|event| event.event_type)
        .collect::<Vec<_>>();
    assert_eq!(
        event_types,
        [
            RunnerEventType::RunnerTokenRotated,
            RunnerEventType::RunnerCordoned,
            RunnerEventType::RunnerOnline,
            RunnerEventType::RunnerRegistered,
        ]
    );

    let filtered = RunnerEventFilter::new(
        vec![
            RunnerEventType::RunnerOnline,
            RunnerEventType::RunnerCordoned,
        ],
        Some(ENROLLED_AT + 1),
        Some(ENROLLED_AT + 2),
    )
    .expect("the inclusive window is ordered");
    let page = fixtures
        .runners()
        .runner_events(runner, &filtered, None, PageLimit::default())
        .await
        .expect("the filtered page loads");
    assert_eq!(page.total(), 2);
    assert_eq!(
        page.items()
            .iter()
            .map(|event| event.event_type)
            .collect::<Vec<_>>(),
        [
            RunnerEventType::RunnerCordoned,
            RunnerEventType::RunnerOnline,
        ]
    );
}
