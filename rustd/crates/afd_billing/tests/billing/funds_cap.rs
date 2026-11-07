//! A funds read that cannot finish within the API login's bounds gives up and
//! says so, rather than holding the renewal or admission that asked.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::{Duration, Instant};

use afd_billing::biller::{Account, Ask, Biller, Funds, OutagePolicy, WithOutagePolicy, WorkKey};
use afd_db::test_util::mint_uuid7;
use jiff::Timestamp;
use sqlx::Connection as _;

use crate::biller::{LINE, ledger};
use crate::lane::Lane;

/// Long enough for the cap on a loaded lane; a read still waiting past it hung.
const BOUND: Duration = Duration::from_secs(10);

/// Takes the lock every read of the usage log waits behind.
const LOCK_LINES: &str = "LOCK TABLE billing.usage_lines IN ACCESS EXCLUSIVE MODE";

/// Postgres ended the lock wait at the login's `lock_timeout`.
const LOCK_NOT_AVAILABLE: &str = "55P03";

/// The login's lock bound, which ends a read waiting on the locked log.
fn cap() -> Duration {
    let millis = afd_db::login::LOCK_TIMEOUT
        .strip_suffix("ms")
        .and_then(|millis| millis.parse().ok())
        .expect("the cap is set in milliseconds");
    Duration::from_millis(millis)
}

/// The SQLSTATE somewhere in `fault`'s causes, if a database answered one.
fn sqlstate(fault: &afd_billing::Error) -> Option<String> {
    let mut cause = std::error::Error::source(fault);
    while let Some(current) = cause {
        if let Some(code) = current
            .downcast_ref::<sqlx::Error>()
            .and_then(sqlx::Error::as_database_error)
            .and_then(sqlx::error::DatabaseError::code)
        {
            return Some(code.into_owned());
        }
        cause = current.source();
    }
    None
}

/// With the usage log locked, a funds check and a balance read each end at
/// the login's lock bound, with an error the outage policy counts as
/// unanswered, and an open policy runs the work. Once the lock goes, the same check answers.
///
/// A private database, because the table lock would stall every other test
/// on the shared lane.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_funds_read_past_its_cap_gives_up_unanswered() {
    let lane = Lane::isolated().await;
    lane.wallet(LINE.as_i64() * 4, None, Timestamp::now()).await;
    let payer = Account {
        tenant: lane.tenant.clone(),
        workspace: lane.workspace.clone(),
    };
    let ask = Ask {
        work: WorkKey::FleetLease(mint_uuid7()),
        payer: payer.clone(),
        next: LINE,
    };
    let mut holder = lane.pool.acquire().await.expect("the lane answers");
    let mut lock = holder
        .begin()
        .await
        .expect("the holder's transaction opens");
    sqlx::query(LOCK_LINES)
        .execute(&mut *lock)
        .await
        .expect("the usage log locks");

    let biller = ledger(&lane);
    let started = Instant::now();
    let refused = tokio::time::timeout(BOUND, biller.check(&ask))
        .await
        .expect("the check gives up on its own")
        .expect_err("a check past its cap fails");
    assert_eq!(
        sqlstate(&refused).as_deref(),
        Some(LOCK_NOT_AVAILABLE),
        "{refused}"
    );
    assert!(refused.is_unanswered(), "{refused}");
    assert!(started.elapsed() >= cap(), "it waited out the cap");
    let unread = tokio::time::timeout(BOUND, biller.balance(&payer))
        .await
        .expect("the balance read gives up on its own")
        .expect_err("a balance read past its cap fails");
    assert_eq!(
        sqlstate(&unread).as_deref(),
        Some(LOCK_NOT_AVAILABLE),
        "{unread}"
    );
    let open = WithOutagePolicy::new(ledger(&lane), OutagePolicy::Open);
    let answered = tokio::time::timeout(BOUND, open.check(&ask))
        .await
        .expect("the open policy answers on its own")
        .expect("an open policy answers");
    assert_eq!(answered, Funds::Unconfirmed);

    lock.rollback().await.expect("the usage log unlocks");
    drop(holder);
    assert_eq!(
        biller.check(&ask).await.expect("the check answers"),
        Funds::Covered
    );
    lane.cleanup().await;
}
