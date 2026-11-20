//! An operator's transitions, edits and reads, against rows the tests enrol.

use std::borrow::Cow;

use af_core::clock;
use af_core::test_util::trace::Capture;
use af_wire::runner::{BindMode, ExtraBind, RunnerLiveness};
use afd_api_wire::admin::{AdminState, RunnerAdminAction, RunnerEventType};
use afd_runner::sql::NEVER_SEEN;
use afd_runner::{DETAIL_SELFTEST_REFUSED, KeysetCursor, RunnerEdit};
use serde_json::Value;

use super::{Fixture, enrolment};
use crate::rows::{self, SetupRunRow};
use af_crypto::entropy::Entropy;
use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::test_util::mint_id;
use afd_db::test_util::{TenantSeed, TestDatabase};
use afd_runner::Runners;

/// Who the operator transitions are recorded as.
const ACTOR: &str = "user_operator";

/// How many runs still name a runner.
const COUNT_RUNS: &str = "SELECT count(*) FROM thread.runs WHERE runner_id = $1";

/// A bind the sandbox validator accepts.
const BIND_PATH: &str = "/srv/models";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_drain_records_its_status_and_a_self_test_ask() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let runners = &fixture.runners;

    let draining = runners
        .platform()
        .transition(&runner, RunnerAdminAction::Drain, ACTOR, clock::now())
        .await
        .expect("a drain applies");
    assert_eq!(draining, AdminState::Draining);
    let history = fixture.history(&runner).await;
    let drained = history.first().expect("the transition is newest");
    assert_eq!(drained.event_type, RunnerEventType::RunnerDraining);
    assert_eq!(
        drained.metadata.get("from_status"),
        Some(&Value::from("active"))
    );
    assert_eq!(
        drained.metadata.get("to_status"),
        Some(&Value::from("draining"))
    );
    assert_eq!(drained.metadata.get("actor_id"), Some(&Value::from(ACTOR)));

    let asked = runners
        .platform()
        .request_selftest(&runner)
        .await
        .expect("a self-test is asked of a live runner");
    assert_eq!(asked.status(), AdminState::Draining);
    let detail = runners
        .platform()
        .runner_detail(&runner)
        .await
        .expect("the detail reads");
    assert_eq!(detail.selftest_requested_at(), Some(asked.requested_at()));

    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_revoke_is_terminal_until_the_record_retires() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let runners = &fixture.runners;

    runners
        .platform()
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    let refused = runners
        .platform()
        .request_selftest(&runner)
        .await
        .expect_err("a revoked runner is asked nothing");
    assert_eq!(refused.detail(), DETAIL_SELFTEST_REFUSED);
    let reopened = runners
        .platform()
        .transition(&runner, RunnerAdminAction::Cordon, ACTOR, clock::now())
        .await
        .expect_err("revoked is terminal");
    assert!(reopened.is_rejected(), "{reopened}");

    runners
        .platform()
        .delete_revoked(&runner)
        .await
        .expect("a revoked runner's record retires");
    let gone = runners
        .self_record(&runner)
        .await
        .expect_err("the retired row is gone");
    assert!(gone.is_runner_vanished(), "{gone}");
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_operator_reads_derive_liveness_and_page_by_creation() {
    let mut fixture = Fixture::create().await;
    let older = fixture.enrol().await;
    let newer = fixture.enrol().await;
    let runners = &fixture.runners;

    let detail = runners
        .platform()
        .runner_detail(&newer)
        .await
        .expect("the detail reads");
    let item = detail.item();
    assert_eq!(item.status(), AdminState::Active);
    assert_eq!(item.liveness(), RunnerLiveness::Registered);
    assert_eq!(item.last_seen_at(), NEVER_SEEN);
    assert_eq!(detail.active_run_count(), 0);
    let registered = fixture.history(&newer).await;
    let enrolment_event = registered.first().expect("the enrolment is recorded");
    assert_eq!(
        enrolment_event.occurred_at,
        item.created_at(),
        "the enrolment event takes the row's own stamp"
    );

    runners
        .heartbeat(&older, &afd_runner::NO_REPORT, None, clock::now())
        .await
        .expect("the older runner beats");
    let beaten = runners
        .platform()
        .runner_detail(&older)
        .await
        .expect("the detail reads");
    assert_eq!(beaten.item().liveness(), RunnerLiveness::Online);

    let boundary = KeysetCursor::new(item.created_at(), item.id().clone());
    let page = runners
        .platform()
        .list_runners(Some(&boundary), 1)
        .await
        .expect("a page after the cursor reads");
    let next = page
        .items()
        .first()
        .expect("the older runner lies behind it");
    assert!(
        (next.created_at(), next.id().as_str()) < (item.created_at(), item.id().as_str()),
        "a page resumes strictly after its cursor"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_edit_stores_its_binds_in_the_sandbox_configuration() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let bind = ExtraBind {
        path: Cow::Borrowed(BIND_PATH),
        mode: BindMode::ReadOnly,
        note: Cow::Borrowed("model cache"),
    };
    let mut policy = enrolment().assigned_policy;
    policy.extra_binds = vec![bind.clone()];
    let labels = [Cow::Borrowed("plane"), Cow::Borrowed("edited")];

    let edited = fixture
        .runners
        .platform()
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: Some(&policy),
                labels: Some(&labels),
            },
            clock::now(),
        )
        .await
        .expect("the edit applies");
    assert_eq!(edited.status, AdminState::Active);

    let stored = fixture
        .runners
        .self_record(&runner)
        .await
        .expect("the edited row reads");
    let decoded = stored.assignment.decode().expect("the assignment decodes");
    assert_eq!(decoded.extra_binds, vec![bind]);
    let history = fixture.history(&runner).await;
    assert_eq!(
        history.first().map(|event| event.event_type),
        Some(RunnerEventType::RunnerPolicyAssigned)
    );
    let detail = fixture
        .runners
        .platform()
        .runner_detail(&runner)
        .await
        .expect("the detail reads");
    assert_eq!(detail.item().labels(), ["plane", "edited"]);
    fixture.cleanup().await;
}

/// The deletion matrix: a revoked runner's record retires even after it ran a
/// setup run, and that run goes with it. A setup run holds no money: its usage
/// lines name the run and outlive it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_retiring_a_runner_that_ran_a_setup_run_removes_its_runs() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let mut connection = fixture.owner.acquire().await.expect("a seed connection");
    rows::setup_run(
        &mut connection,
        (&runner, &mint_id(), &mint_id()),
        SetupRunRow {
            status: afd_runner::sql::RUN_STATUS_EXPIRED,
            run_expires_at: clock::now(),
        },
    )
    .await;

    fixture
        .runners
        .platform()
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    fixture
        .runners
        .platform()
        .delete_revoked(&runner)
        .await
        .expect("a runner that ran a setup run still retires");

    let left: i64 = sqlx::query_scalar(COUNT_RUNS)
        .bind(&runner)
        .fetch_one(&mut *connection)
        .await
        .expect("the retired runner's runs read");
    assert_eq!(left, 0, "a setup run goes with the runner that held it");
    drop(connection);
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_edit_carrying_nothing_is_refused_before_any_write() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let before = fixture.history(&runner).await.len();

    let refused = fixture
        .runners
        .platform()
        .edit(
            &runner,
            RunnerEdit {
                assigned_policy: None,
                labels: None,
            },
            clock::now(),
        )
        .await
        .expect_err("an edit with no policy and no labels changes nothing");

    assert!(refused.is_rejected(), "{refused}");
    assert_eq!(
        fixture.history(&runner).await.len(),
        before,
        "a refused edit appends no event"
    );
    fixture.cleanup().await;
}

/// A rotation answers the new credential once and records it; a revoked
/// runner's is refused inside the same statement, so it records nothing.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_rotation_lands_for_a_live_runner_and_is_refused_once_revoked() {
    let mut fixture = Fixture::create().await;
    let runner = fixture.enrol().await;
    let capture = Capture::install();

    fixture
        .runners
        .platform()
        .rotate_token(&runner, ACTOR, clock::now())
        .await
        .expect("a live runner rotates");
    let rotated = capture.only("runner_token_rotated");
    assert_eq!(rotated.field("runner_id"), Some(runner.as_str()));
    let history = fixture.history(&runner).await;
    assert_eq!(
        history.first().map(|event| event.event_type),
        Some(RunnerEventType::RunnerTokenRotated)
    );

    fixture
        .runners
        .platform()
        .transition(&runner, RunnerAdminAction::Revoke, ACTOR, clock::now())
        .await
        .expect("a revoke applies");
    let refused = fixture
        .runners
        .platform()
        .rotate_token(&runner, ACTOR, clock::now())
        .await
        .expect_err("a revoked runner holds no credential to rotate");

    assert!(refused.is_rejected(), "{refused}");
    assert_eq!(
        fixture
            .history(&runner)
            .await
            .first()
            .map(|event| event.event_type),
        Some(RunnerEventType::RunnerRevoked),
        "the refused rotation appended nothing after the revoke"
    );
    drop(capture);
    fixture.cleanup().await;
}

// Value: protects=a runner page never reports a total that disagrees with the
// rows it lists; fails_when=the total and the rows read two moments, so a
// runner registered between them is listed but not counted; why_new=tenant
// runners register while their owners read; seam=a lock that parks the page
// between its two reads, on a database of this test's own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runner_page_and_its_total_read_one_moment_while_runners_register() {
    let lane = TestDatabase::create().await;
    let owner = lane.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&owner)
        .await
        .expect("the private database migrates");
    let tenant = mint_id();
    TenantSeed::new(&tenant)
        .insert(&mut *owner.acquire().await.expect("an owner connection"))
        .await
        .expect("the tenant seeds");
    let database = lane.open(DbRole::Api, &[]).await;
    let runners = Runners::new(database.clone(), Entropy::new());
    let register = || {
        let (runners, tenant) = (&runners, &tenant);
        async move {
            let (operation, request) = (mint_id(), enrolment());
            runners
                .register_tenant(
                    tenant,
                    &operation,
                    &request,
                    "user_tenant_owner",
                    clock::now(),
                )
                .await
        }
    };
    register().await.expect("the first runner registers");

    // The page's rows join `thread.runs` and its total does not, so holding
    // that table parks the page after its total and before its rows.
    let mut holder = owner.acquire().await.expect("a holding connection");
    let mut hold = sqlx::Acquire::begin(&mut *holder).await.expect("a hold");
    sqlx::query("LOCK TABLE thread.runs IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *hold)
        .await
        .expect("the lock");
    let page = tokio::spawn({
        let runners = runners.clone();
        let tenant = tenant.clone();
        async move { runners.tenant(&tenant).list_runners(None, 200).await }
    });
    let waiting = "SELECT count(*) FROM pg_locks \
                   WHERE NOT granted AND relation = 'thread.runs'::regclass";
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let parked: i64 = sqlx::query_scalar(waiting)
                .fetch_one(&mut *owner.acquire().await.expect("a probe connection"))
                .await
                .expect("pg_locks reads");
            if parked > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the page parks on the held table");

    register()
        .await
        .expect("a second runner registers while the page is parked");
    hold.rollback().await.expect("the hold releases");
    drop(holder);
    let page = page.await.expect("the page joins").expect("the page reads");
    let listed = i64::try_from(page.items().len()).expect("a page fits an i64");
    assert_eq!(
        (page.total(), listed),
        (1, 1),
        "the total and the rows both read the moment before the second registration"
    );
    let after = runners
        .tenant(&tenant)
        .list_runners(None, 200)
        .await
        .expect("a later page");
    assert_eq!(after.total(), 2, "a later page counts both");

    drop(runners);
    database.close().await;
    owner.close().await;
    lane.cleanup().await;
}

/// Both registration paths name who registered the runner on its first event,
/// as every other runner event names its actor.
// Value: protects=the audit trail names the person who added a host;
// fails_when=a registration path drops the caller's subject; why_new=Indy
// chose to record the registrant in this PR; seam=the store on live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_registration_event_names_who_registered_the_runner() {
    let mut fixture = Fixture::create().await;
    let platform = fixture.enrol().await;

    let tenant = mint_id();
    TenantSeed::new(&tenant)
        .insert(&mut *fixture.owner.acquire().await.expect("an owner connection"))
        .await
        .expect("the tenant seeds");
    let registered = fixture
        .runners
        .register_tenant(
            &tenant,
            &mint_id(),
            &enrolment(),
            "user_tenant_owner",
            clock::now(),
        )
        .await
        .expect("the tenant registers");
    assert!(
        matches!(registered, afd_runner::Registered::Created(_)),
        "a fresh key registers a new runner"
    );
    let tenant_runner = match registered {
        afd_runner::Registered::Created(enrolled) => enrolled.runner_id,
        afd_runner::Registered::Replayed { runner_id, .. } => runner_id,
    };
    fixture.enrolled.push(tenant_runner.clone());

    for (runner, actor) in [
        (&platform, "user_operator"),
        (&tenant_runner, "user_tenant_owner"),
    ] {
        let history = fixture.history(runner).await;
        let registered = history
            .last()
            .expect("the registration event is the oldest");
        assert_eq!(registered.event_type, RunnerEventType::RunnerRegistered);
        assert_eq!(
            registered.metadata.get("actor_id").and_then(Value::as_str),
            Some(actor),
            "the registration names who registered {runner}"
        );
    }
    fixture.cleanup().await;
}
