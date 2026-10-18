use afd_db::error::database_answer;

use super::{OutagePolicy, WithOutagePolicy};
use crate::Result;
use crate::biller::{
    Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use crate::error::query;

/// A biller whose `reserve` always fails the way it was built to.
#[derive(Debug)]
struct Failing(fn() -> sqlx::Error);

#[async_trait::async_trait]
impl Biller for Failing {
    async fn reserve(&self, _ask: &Reservation) -> Result<Hold> {
        Err(query("hold a reservation")((self.0)()))
    }

    async fn settle(&self, _settlement: &Settlement) -> Result<Receipt> {
        Ok(Receipt::default())
    }

    async fn release(&self, _work: &WorkKey, _why: Release) -> Result<()> {
        Ok(())
    }

    async fn balance(&self, _payer: &Account) -> Result<Balance> {
        Err(query("read a balance")((self.0)()))
    }
}

fn ask() -> Reservation {
    let id = afd_db::test_util::mint_uuid7();
    Reservation {
        work: WorkKey::FleetLease(id.clone()),
        payer: Account {
            tenant: id.clone(),
            workspace: id,
        },
        ceiling: crate::Nanos::from_i64(1),
    }
}

fn reset() -> sqlx::Error {
    sqlx::Error::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "reset by peer",
    ))
}

fn unique_violation() -> sqlx::Error {
    database_answer("23505")
}

#[tokio::test]
async fn a_connection_lost_mid_query_is_an_outage_the_policy_decides() {
    let open = WithOutagePolicy::new(Failing(reset), OutagePolicy::Open);
    assert_eq!(open.reserve(&ask()).await.ok(), Some(Hold::Unconfirmed));
    let closed = WithOutagePolicy::new(Failing(reset), OutagePolicy::Closed);
    let refused = closed.reserve(&ask()).await.err();
    assert!(refused.is_some_and(|fault| fault.is_datastore_unavailable()));
}

#[tokio::test]
async fn a_statement_defect_is_refused_under_either_policy() {
    for policy in [OutagePolicy::Open, OutagePolicy::Closed] {
        let biller = WithOutagePolicy::new(Failing(unique_violation), policy);
        let refused = biller.reserve(&ask()).await.err();
        assert!(
            refused.is_some_and(|fault| !fault.is_datastore_unavailable()),
            "a constraint failure is a defect, refused under {policy:?}"
        );
    }
}

fn lock_wait_expired() -> sqlx::Error {
    database_answer("55P03")
}

fn statement_cancelled() -> sqlx::Error {
    database_answer("57014")
}

/// Contention on the wallet lock is the datastore answering "not now": not a
/// lost connection, but still a biller with no answer, so the outage policy
/// decides it exactly as it decides an outage.
#[tokio::test]
async fn a_lock_wait_past_its_timeout_is_contention_the_policy_decides() {
    for contended in [
        lock_wait_expired as fn() -> sqlx::Error,
        statement_cancelled,
    ] {
        let open = WithOutagePolicy::new(Failing(contended), OutagePolicy::Open);
        assert_eq!(open.reserve(&ask()).await.ok(), Some(Hold::Unconfirmed));

        let closed = WithOutagePolicy::new(Failing(contended), OutagePolicy::Closed);
        let refused = closed.reserve(&ask()).await.err();
        assert!(refused.is_some_and(|fault| {
            fault.is_contended() && fault.is_unanswered() && !fault.is_datastore_unavailable()
        }));
    }
}
