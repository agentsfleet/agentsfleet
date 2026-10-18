//! Report replay outlives private text; settlement and debit share one transaction.

mod prepare;
mod receipt;
mod retained;
mod settle;
mod sql;
pub(crate) mod verdict;

use afd_core::id::Uuid7;
use afd_wire::authoring::{Report, Reported};
use sqlx::{Connection as _, PgConnection};

use super::{Status, guard, meter};
use crate::{
    Drafts, Result,
    error::{ErrorKind, raise::query},
    funnel::Owed,
};

pub(super) const CONTEXT: &str = "settle authoring accounting report";

impl Drafts {
    /// Records a bounded terminal report or content-free accounting recovery.
    ///
    /// A replay of an acknowledgement saved while usage was unknown settles
    /// once the dispatch receipts it waited on become known.
    ///
    /// # Errors
    /// Refuses foreign/bad fences, changed report identity and invented meters.
    /// Accounting replay remains available after private deletion or expiry.
    pub async fn report(&self, runner: &Uuid7, lease: &Uuid7, request: Report) -> Result<Reported> {
        let prepared = prepare::Prepared::new(request)?;
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let mut owed = Owed::default();
        let private = prepare::publication(&mut tx, runner, lease, &prepared, &mut owed).await?;
        let attempt = guard::accounting(&mut tx, runner, lease, &prepared.counters).await?;
        let ack = match receipt::find(&mut tx, &attempt, &prepared).await? {
            Some(saved) if saved.settled => saved,
            Some(saved) => {
                retained::resolve(self, &mut tx, &attempt, &prepared, saved, &mut owed).await?
            }
            None if attempt.status == Status::Settled => return Err(ErrorKind::Replay.into()),
            None => {
                Basis::measure(&mut tx, &attempt, &prepared, private.as_ref())
                    .await?
                    .conclude(self, &mut tx, settle::Acknowledgement::First, &mut owed)
                    .await?
            }
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        owed.settle(&self.analytics);
        Ok(ack)
    }
}

/// What a settlement decides from, sampled under the attempt row lock.
struct Basis<'a> {
    attempt: &'a guard::Attempt,
    prepared: &'a prepare::Prepared,
    private: Option<&'a guard::Guarded>,
    now: jiff::Timestamp,
    verified: meter::Verified,
}

impl<'a> Basis<'a> {
    async fn measure(
        connection: &mut PgConnection,
        attempt: &'a guard::Attempt,
        prepared: &'a prepare::Prepared,
        private: Option<&'a guard::Guarded>,
    ) -> Result<Self> {
        let now = crate::sql::clock(connection, CONTEXT).await?;
        let observed =
            garde::Unvalidated::new(prepared.request.accounting().usage.clone()).validate()?;
        let verified = meter::verify(connection, attempt, &observed, now).await?;
        Ok(Self {
            attempt,
            prepared,
            private,
            now,
            verified,
        })
    }

    async fn conclude(
        self,
        drafts: &Drafts,
        connection: &mut PgConnection,
        acknowledgement: settle::Acknowledgement,
        owed: &mut Owed,
    ) -> Result<Reported> {
        let cancelled = guard::cancelled(connection, self.attempt).await?;
        let verdict = verdict::derive(
            self.attempt,
            self.prepared,
            &self.verified,
            self.private,
            cancelled,
            self.now,
        )?;
        settle::commit(
            drafts,
            connection,
            settle::Settlement {
                attempt: self.attempt,
                prepared: self.prepared,
                verified: &self.verified,
                verdict: &verdict,
                private: self.private,
                now: self.now,
                acknowledgement,
            },
            owed,
        )
        .await
    }
}
