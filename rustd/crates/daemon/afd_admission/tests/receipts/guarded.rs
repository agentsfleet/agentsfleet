//! The guarded admission: a write that goes ahead only while its guard holds,
//! proved inside the admission's own transaction.

use std::pin::Pin;

use afd_admission::{Reply as ReplyTo, Unheld};
use afd_db::Precondition;
use afd_db::test_util::mint_id;
use sqlx::PgConnection;

use super::{Fallible, lane::Lane, ledger::steer, queue::over_lane};

/// How many admissions the lane's fleet holds.
const ADMISSION_ROWS: &str = "SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1";

/// A guard that answers its verdict every time it is asked.
struct Verdict(bool);

impl Precondition for Verdict {
    fn holds<'c>(
        &'c self,
        _connection: &'c mut PgConnection,
    ) -> Pin<Box<dyn Future<Output = sqlx::Result<bool>> + Send + 'c>> {
        Box::pin(std::future::ready(Ok(self.0)))
    }
}

async fn rows(lane: &Lane) -> Fallible<i64> {
    Ok(sqlx::query_scalar(ADMISSION_ROWS)
        .bind(&lane.fleet)
        .fetch_one(&mut *lane.database.acquire().await?)
        .await?)
}

/// M252 Dimension 2.5. A guard that fails answers `Unheld` and writes no row;
/// one that holds writes the row a runnable admission writes.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn guarded_admission_writes_nothing_when_the_guard_fails() -> Fallible {
    let lane = Lane::seed().await?;
    let admissions = over_lane(&lane).await?;
    sqlx::query("UPDATE fleet.fleets SET status = 'active' WHERE id = $1")
        .bind(&lane.fleet)
        .execute(&mut *lane.database.acquire().await?)
        .await?;
    let before = rows(&lane).await?;

    let key = mint_id();
    let refused = admissions
        .admit_runnable_guarded(steer(&lane, key.as_str(), ReplyTo::None), &Verdict(false))
        .await?;
    assert_eq!(refused.err(), Some(Unheld), "a failed guard admits nothing");
    assert_eq!(rows(&lane).await?, before, "a failed guard writes no row");

    let admitted = admissions
        .admit_runnable_guarded(steer(&lane, key.as_str(), ReplyTo::None), &Verdict(true))
        .await?
        .map_err(|_unheld| "a guard that holds admits")?;
    assert!(
        !admitted.replayed,
        "the refused attempt left no row to replay"
    );
    assert_eq!(rows(&lane).await?, before + 1);
    let stored: String = sqlx::query_scalar("SELECT producer FROM fleet.admissions WHERE id = $1")
        .bind(&admitted.stored.id)
        .fetch_one(&mut *lane.database.acquire().await?)
        .await?;
    assert_eq!(stored, "steer");
    lane.cleanup().await;
    Ok(())
}
