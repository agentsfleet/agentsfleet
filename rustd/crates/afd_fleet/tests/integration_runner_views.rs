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
use afd_core::error_code;
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

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runner_views_report_missing_and_malformed_rows_without_partial_success() {
    let fixtures = Fixtures::create().await;
    let missing = Uuid7::parse("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0b")
        .expect("the missing identifier is canonical");
    let detail_error = fixtures
        .runners()
        .runner_detail(&missing, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect_err("a missing runner has no detail");
    let event_error = fixtures
        .runners()
        .runner_events(
            &missing,
            &RunnerEventFilter::default(),
            None,
            PageLimit::default(),
        )
        .await
        .expect_err("a missing runner has no history");
    for error in [detail_error, event_error] {
        assert_eq!(error.code(), error_code::RUNNER_NOT_FOUND);
        assert_eq!(error.detail(), afd_runner::DETAIL_RUNNER_NOT_FOUND);
    }

    let enrolled = fixtures
        .runners()
        .register(
            &enrolment(SandboxTier::DevNone, NetworkPolicy::AllowAll, 1),
            UnixMillis::from_millis(ENROLLED_AT),
        )
        .await
        .expect("the runner enrols");
    overwrite_admin_state(&fixtures, &enrolled.runner_id).await;
    let malformed = fixtures
        .runners()
        .runner_detail(&enrolled.runner_id, UnixMillis::from_millis(ENROLLED_AT))
        .await
        .expect_err("an unknown stored state fails the whole detail");
    assert_eq!(malformed.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(malformed.detail(), afd_runner::DETAIL_DATABASE_ERROR);
    // Restored the moment the assertion is made, and not at the end of the
    // test. `fleet.runners` is shared with every other suite in this lane, and
    // an undecodable `admin_state` fails a WHOLE listing rather than the row
    // that carries it — so while this fixture is stored, any unfiltered
    // `list_runners` anywhere in the binary refuses. The window is narrowed to
    // the two statements it takes to prove the refusal.
    restore_admin_state(&fixtures, &enrolled.runner_id).await;
    insert_unknown_event(&fixtures, &enrolled.runner_id).await;
    let malformed_event = fixtures
        .runners()
        .runner_events(
            &enrolled.runner_id,
            &RunnerEventFilter::default(),
            None,
            PageLimit::default(),
        )
        .await
        .expect_err("an unknown stored event type fails the whole page");
    assert_eq!(malformed_event.code(), error_code::INTERNAL_DB_QUERY);
    assert_eq!(malformed_event.detail(), afd_runner::DETAIL_DATABASE_ERROR);
    assert!(std::error::Error::source(&malformed_event).is_some());
    fixtures.cleanup().await;
}

async fn overwrite_admin_state(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.runners SET admin_state = $2 WHERE id = $1::uuid")
        .bind(runner.as_str())
        .bind("unknown_state")
        .execute(&mut *connection)
        .await
        .expect("the malformed fixture state is stored");
}

/// Puts a decodable state back, so the shared listing reads again.
async fn restore_admin_state(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("UPDATE fleet.runners SET admin_state = $2 WHERE id = $1::uuid")
        .bind(runner.as_str())
        .bind("active")
        .execute(&mut *connection)
        .await
        .expect("the fixture state is restored");
}

async fn insert_unknown_event(fixtures: &Fixtures, runner: &Uuid7) {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO fleet.runner_events \
         (id, runner_id, event_type, metadata, created_at) \
         VALUES ($1::uuid, $2::uuid, $3, $4::jsonb, $5)",
    )
    .bind("0199a6f0-1c2d-7e3f-8a4b-5c6d7e8f9a0c")
    .bind(runner.as_str())
    .bind("unknown_event")
    .bind("{}")
    .bind(ENROLLED_AT + 4)
    .execute(&mut *connection)
    .await
    .expect("the malformed fixture event is stored");
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

/// Walks the whole keyset listing and grades it against the seeded runners.
///
/// # Why this does not assert a total
///
/// It used to read `assert_eq!((first.total(), second.total()), (3, 3))`, which
/// holds only if this test OWNS the database. It does not: `Fixtures::create`
/// takes `TestDatabase::shared`, and `afd_db::test_util` reserves the
/// per-test database for the migrator suites alone. Every other integration
/// file enrols runners into the same rows, so the global count is whatever the
/// lane happens to have run — sixty-five when M178's suites joined M179's, and
/// three only while this file was nearly the only writer.
///
/// What the dimension is actually about survives, and is graded harder: the
/// composite cursor must walk the whole set skipping no tie and repeating no
/// row, every page must account for the seeded runners, and those runners must
/// come back in their seeded order with their derived liveness.
///
/// # Why it does not assert the total HOLDS STILL either
///
/// It also used to assert every page reported the same total, and that clause
/// outlived the count it replaced for the same reason: it was true of the test
/// environment, not of the code. Each page is its own query and so its own MVCC
/// snapshot, so a runner enrolled between two pages legitimately changes the
/// count — Postgres promises consistency WITHIN a statement, never across a
/// walk. The clause passed only while `cargo` ran this file as its own binary,
/// which serialised it against every sibling that writes here. Aggregating the
/// suites into one binary runs them concurrently and the total moved, exactly
/// as the database allows: `[68, 68, ..., 68, 69]`.
///
/// Asserting it back would be pinning an accident of test scheduling. What
/// keyset pagination actually guarantees under a concurrent writer is below,
/// and is the stronger claim: the walk skips no seeded row, repeats none, and
/// every page's total accounts for the seeded set.
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
        // Stops at the seeded set rather than walking to the end. The table is
        // shared with every other suite in this lane, so a full walk reads
        // rows this test did not write — including the deliberately
        // undecodable `admin_state` its sibling stores — and none of them is
        // what this dimension is about.
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
