//! The capacity handoff is serialized before an expiring report decides its hold.

use super::{report::processed, thread};
use crate::{
    report_seed::{DEEP_POOL, Held, held_in},
    seed::{self, ACTOR, MODEL, POSTURE, PROVIDER, REQUEST_JSON},
    support::Fixtures,
};
use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_fleet::run::{Acquired, Billed, Issued};
use jiff_sqlx::ToSqlx as _;
use sqlx::{Connection as _, Postgres, Transaction};
use std::time::Duration;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Next {
    acquired: Acquired,
    tenant: Uuid7,
}

// Pause once the report has decided its hold and before the statement's audit FK check.
const HOLD_TRIGGER: &str = "
CREATE FUNCTION runner.pause_hold_fixture() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended(NEW.fleet_id::text, 0));
    RETURN NEW;
END $$;
CREATE TRIGGER pause_hold_fixture BEFORE UPDATE OF hold_expires_at ON thread.threads
FOR EACH ROW WHEN (NEW.hold_expires_at IS NOT NULL)
EXECUTE FUNCTION runner.pause_hold_fixture()";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_hold_decided_before_expiry_keeps_the_worker_until_the_report_commits() -> Checked {
    let held = held_in(Fixtures::create_isolated().await).await;
    let next = next_turn(&held).await?;
    let migrator = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = migrator.acquire().await?;
    sqlx::raw_sql(HOLD_TRIGGER)
        .execute(&mut *connection)
        .await?;
    let mut blocker = connection.begin().await?;
    let pid = pause_hold(&held, &mut blocker).await?;
    let expiry = shorten_run(&held).await?;
    let plane = held.fixtures.plane();
    let request = processed(&held, Some(held.fixtures.after(Duration::from_mins(5))));
    let mut clock = af_observability::verb_span::VerbClock::untimed();

    let (reported, attempted) = tokio::join!(
        plane.report_with(
            &held.runner,
            &request,
            af_core::clock::now(),
            None,
            &mut clock
        ),
        issue_during_report(&held, &next, blocker, pid, expiry)
    );
    let report = reported?;
    let (waited, issued) = attempted?;
    let recorded = thread(&held.fixtures, &held.fleet).await.0;
    drop(connection);
    migrator.close().await;
    held.fixtures.cleanup().await;

    assert!(
        waited,
        "issue must wait for the host lock before counting capacity"
    );
    assert!(
        issued.is_none(),
        "one worker cannot take a second fleet alongside the hold"
    );
    let receipt = report.hold.ok_or("the live report must retain its hold")?;
    assert_eq!(recorded, Some(receipt.expires_at));
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_expired_report_can_settle_after_another_fleet_takes_the_worker() -> Checked {
    let held = held_in(Fixtures::create_isolated().await).await;
    let next = next_turn(&held).await?;
    let expiry = shorten_run(&held).await?;
    wait_for_expiry(&held, expiry).await?;
    let issued = issue(&held, &next).await?;
    assert!(issued.is_some(), "expired run frees the only worker");
    let plane = held.fixtures.plane();
    let request = processed(&held, Some(held.fixtures.after(Duration::from_mins(5))));
    let report = plane
        .report_with(
            &held.runner,
            &request,
            af_core::clock::now(),
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
        )
        .await?;
    assert!(report.hold.is_none(), "late settlement cannot add a hold");
    assert_eq!(thread(&held.fixtures, &held.fleet).await.0, None);
    held.fixtures.cleanup().await;
    Ok(())
}

async fn next_turn(held: &Held) -> Checked<Next> {
    let fixtures = &held.fixtures;
    let seeded = seed::seeded_for::<0>(fixtures, ACTOR, REQUEST_JSON).await;
    let mut connection = fixtures.database.acquire().await?;
    sqlx::query(
        "UPDATE fleet.fleets SET required_tags =
            (SELECT required_tags FROM fleet.fleets WHERE id=$2) WHERE id=$1",
    )
    .bind(&seeded.fleet)
    .bind(&held.fleet)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE runner.runners SET worker_count=1 WHERE id=$1")
        .bind(&held.runner)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    fixtures.seed_wallet(&seeded.tenant, DEEP_POOL).await;
    let acquired = seed::select_fleet(&held.runs, &held.runner, &seeded.fleet)
        .await
        .ok_or("second fleet is not provisionally assignable")?;
    Ok(Next {
        acquired,
        tenant: seeded.tenant,
    })
}

async fn pause_hold(held: &Held, blocker: &mut Transaction<'_, Postgres>) -> Checked<i32> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(held.fleet.as_str())
        .execute(&mut **blocker)
        .await?;
    Ok(sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut **blocker)
        .await?)
}

async fn shorten_run(held: &Held) -> Checked<jiff_sqlx::Timestamp> {
    let mut connection = held.fixtures.database.acquire().await?;
    let expires: jiff_sqlx::Timestamp = sqlx::query_scalar(
        "UPDATE thread.runs SET run_expires_at=clock_timestamp()+interval '1400 milliseconds'
         WHERE id=$1 RETURNING run_expires_at",
    )
    .bind(&held.issued.run_id)
    .fetch_one(&mut *connection)
    .await?;
    sqlx::query("UPDATE thread.threads SET run_expires_at=$2 WHERE fleet_id=$1")
        .bind(&held.fleet)
        .bind(expires.to_jiff().to_sqlx())
        .execute(&mut *connection)
        .await?;
    Ok(expires)
}

async fn issue_during_report(
    held: &Held,
    next: &Next,
    blocker: Transaction<'_, Postgres>,
    pid: i32,
    expiry: jiff_sqlx::Timestamp,
) -> Checked<(bool, Option<Issued>)> {
    let report = waiting_on(held, pid).await?;
    wait_for_expiry(held, expiry).await?;
    let issuing = issue(held, next);
    tokio::pin!(issuing);
    let finished = tokio::select! {
        issued = &mut issuing => Some(issued),
        waiter = waiting_on(held, report) => {
            waiter?;
            None
        }
    };
    blocker.rollback().await?;
    match finished {
        Some(issued) => Ok((false, issued?)),
        None => Ok((true, issuing.await?)),
    }
}

async fn issue(held: &Held, next: &Next) -> Checked<Option<Issued>> {
    Ok(held
        .runs
        .issue(
            &held.runner,
            &next.acquired,
            Billed {
                tenant_id: &next.tenant,
                posture: POSTURE,
                provider: PROVIDER,
                model: MODEL,
            },
            af_core::clock::now(),
        )
        .await?)
}

async fn wait_for_expiry(held: &Held, expires: jiff_sqlx::Timestamp) -> Checked {
    let mut observer = held.fixtures.database.acquire().await?;
    sqlx::query("SELECT pg_sleep_until($1::timestamptz + interval '1 millisecond')")
        .bind(expires.to_jiff().to_sqlx())
        .execute(&mut *observer)
        .await?;
    Ok(())
}

async fn waiting_on(held: &Held, blocker: i32) -> Checked<i32> {
    let mut observer = held.fixtures.database.acquire().await?;
    Ok(tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(pid) = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid))
                 AND datname=current_database() ORDER BY pid LIMIT 1",
            )
            .bind(blocker)
            .fetch_optional(&mut *observer)
            .await?
            {
                return Ok::<i32, sqlx::Error>(pid);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await??)
}
