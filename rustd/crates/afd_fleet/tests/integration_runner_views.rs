//! Operator runner reads against live Postgres.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_requests.rs"]
mod requests;

#[path = "support/view_heartbeat.rs"]
mod view_heartbeat;

use afd_core::clock::UnixMillis;
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_runner::{PageLimit, RunnerEventFilter};
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

async fn assert_runner_pages(fixtures: &Fixtures, seeded: &SeededViews) {
    let limit = PageLimit::new(2).expect("two is a valid page limit");
    let now = UnixMillis::from_millis(ENROLLED_AT + 4);
    let first = fixtures
        .runners()
        .list_runners(None, limit, now)
        .await
        .expect("the first page loads");
    let second = fixtures
        .runners()
        .list_runners(first.next_cursor(), limit, now)
        .await
        .expect("the second page loads");
    assert_eq!((first.total(), second.total()), (3, 3));
    assert!(second.next_cursor().is_none());
    let items = first
        .into_items()
        .into_iter()
        .chain(second.into_items())
        .collect::<Vec<_>>();
    let ids = items
        .iter()
        .map(|item| item.id().as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        ids, seeded.ordered_ids,
        "the composite cursor skips no ties"
    );
    for item in items {
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
