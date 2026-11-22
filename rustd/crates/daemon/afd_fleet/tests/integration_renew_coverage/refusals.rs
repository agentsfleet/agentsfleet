//! What a fleet run's renewal refuses before and under its locks: a run that is
//! not this runner's, one a Stop marked, and one a Stop or a removal reached
//! while the renewal waited for the run's lock.

use std::time::Duration;

use af_core::error_code;
use af_core::id::Uuid7;
use af_wire::report::RenewRequest;
use af_wire::usage::{LineKind, RequestLines, RequestUsage};
use jiff::Timestamp;
use sqlx::Connection as _;

use super::{DEEP_DAILY_BUDGET, set_fleet_config};
use crate::report_seed::{DEEP_POOL, Held, held};
use crate::rows::Row;

/// The input tokens the runner's one reported request read.
const INPUT_TOKENS: u32 = 900;

/// How long a race waits for the renewal to reach the held row.
const WAIT_LIMIT: Duration = Duration::from_secs(5);

/// A renewal carrying one request the runner made since its last renewal.
fn one_request() -> RenewRequest {
    RenewRequest {
        requests: RequestLines(vec![RequestUsage {
            seq: 0,
            at: af_core::clock::now(),
            input_tokens: INPUT_TOKENS,
            cached_input_tokens: 0,
            cache_write_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            output_tokens: 0,
        }]),
    }
}

/// A held run whose payer and fleet have room, so only the case's own change
/// decides its renewal.
async fn funded() -> Held {
    let held = held().await;
    held.fixtures.seed_wallet(&held.tenant, DEEP_POOL).await;
    set_fleet_config(&held, DEEP_DAILY_BUDGET).await;
    held
}

async fn renew(held: &Held, run: &Uuid7) -> afd_fleet::Result<(Timestamp, afd_billing::Nanos)> {
    held.fixtures
        .plane()
        .renew(&held.runner, run, one_request(), held.now)
        .await
}

/// The model lines logged against `run`, as each line's input tokens.
async fn model_tokens(held: &Held, run: &Uuid7) -> Vec<i64> {
    sqlx::query_scalar(
        "SELECT input_token_count FROM billing.usage_lines \
         WHERE work_id = $1 AND kind = $2 ORDER BY seq",
    )
    .bind(run)
    .bind(LineKind::Model.as_str())
    .fetch_all(&mut *held.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the run's model lines")
}

async fn deadline(held: &Held) -> Option<Timestamp> {
    held.fixtures
        .instant(Row::Run(&held.issued.run_id), "run_expires_at")
        .await
}

/// Renews the held run while another transaction holds its row, running
/// `statement` on it there once the renewal waits, then committing.
async fn renew_while(
    held: &Held,
    statement: &'static str,
) -> afd_fleet::Result<(Timestamp, afd_billing::Nanos)> {
    let run = &held.issued.run_id;
    let database = &held.fixtures.database;
    let mut connection = database.acquire().await.expect("a blocking connection");
    let mut blocking = connection.begin().await.expect("the blocker opens");
    let blocker: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM thread.runs WHERE id = $1 FOR UPDATE")
            .bind(run)
            .fetch_one(&mut *blocking)
            .await
            .expect("the run row is held");
    let change = async {
        tokio::time::timeout(WAIT_LIMIT, async {
            let mut observer = database.acquire().await.expect("an observer");
            while !sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                 WHERE $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(&mut *observer)
            .await
            .expect("the lock graph reads")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the renewal reaches the held run");
        let changed = sqlx::query(statement)
            .bind(run)
            .execute(&mut *blocking)
            .await
            .expect("the held run changes");
        assert_eq!(changed.rows_affected(), 1, "{statement}");
        blocking.commit().await.expect("the blocker commits");
    };
    let (renewed, ()) = tokio::join!(renew(held, run), change);
    renewed
}

// Value: protects=a runner cannot renew a run it does not hold; fails_when=the
// renewal's load stops scoping by runner, or a missing row is renewed;
// why_new=the fleet renewal's not-found arm had no case; seam=RunService::renew
// over live Postgres, naming a run nobody issued.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_renewal_naming_no_run_of_this_runner_is_not_found() {
    let held = funded().await;
    let before = deadline(&held).await;

    let answer = renew(&held, &afd_db::test_util::mint_id()).await;

    assert_eq!(
        answer.expect_err("an unknown run is refused").code(),
        error_code::RUN_NOT_FOUND
    );
    assert_eq!(deadline(&held).await, before, "the held run is untouched");
    held.fixtures.cleanup().await;
}

// Value: protects=a Stop the runner never heard ends the run at its next renewal,
// and the usage it reported is billed; fails_when=the cancel mark is read after
// the gates or not at all, or the refusal drops the request lines; why_new=the
// pre-lock cancelled arm had no service-level case; seam=RunService::renew over
// live Postgres, the run's cancel mark set.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_marked_runs_renewal_is_cancelled_and_its_request_lines_are_logged() {
    let held = funded().await;
    let run = &held.issued.run_id;
    sqlx::query("UPDATE thread.runs SET cancel_requested_at = now() WHERE id = $1")
        .bind(run)
        .execute(&mut *held.fixtures.database.acquire().await.expect("writer"))
        .await
        .expect("the cancel mark is set");
    let before = deadline(&held).await;

    let answer = renew(&held, run).await;

    assert_eq!(
        answer.expect_err("a marked run is refused").code(),
        error_code::RUN_CANCELLED
    );
    assert_eq!(deadline(&held).await, before, "the run is not extended");
    assert_eq!(model_tokens(&held, run).await, [i64::from(INPUT_TOKENS)]);
    held.fixtures.cleanup().await;
}

// Value: protects=a Stop committed while the renewal waits for the run's lock is
// what the runner hears; fails_when=the service maps the locked re-read's cancel to
// lost or extends; why_new=the service's cancelled verdict had no case;
// seam=RunService::renew over live Postgres, the run row held while the mark is set.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stop_committed_while_the_renewal_waits_is_answered_cancelled() {
    let held = funded().await;
    let before = deadline(&held).await;

    let answer = renew_while(
        &held,
        "UPDATE thread.runs SET cancel_requested_at = now() WHERE id = $1",
    )
    .await;

    assert_eq!(
        answer.expect_err("the Stop is the answer").code(),
        error_code::RUN_CANCELLED
    );
    assert_eq!(deadline(&held).await, before, "the run is not extended");
    assert_eq!(
        model_tokens(&held, &held.issued.run_id).await,
        [i64::from(INPUT_TOKENS)]
    );
    held.fixtures.cleanup().await;
}

// Value: protects=a run removed while its renewal waits is lost, not extended and
// not a fault; fails_when=the locked re-read's missing row reads as live or as a
// fault; why_new=the vanished-run arm had no case; seam=RunService::renew over live
// Postgres, the run row deleted while held, as a runner's removal cascades it;
// no_claim=the reported request lines, which a fleet run's refusal prices from the
// run row this removal took.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_run_removed_while_its_renewal_waits_is_lost() {
    let held = funded().await;

    let answer = renew_while(&held, "DELETE FROM thread.runs WHERE id = $1").await;

    assert_eq!(
        answer.expect_err("a removed run is refused").code(),
        error_code::RUN_LOST
    );
    assert_eq!(deadline(&held).await, None, "the run stays removed");
    held.fixtures.cleanup().await;
}
