//! Report replay outlives private text; a settlement's usage lines commit with it,
//! and its money moves through the biller after commit.

mod prepare;
mod receipt;
mod retained;
mod settle;
mod sql;
mod usage;
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
        let (ack, owing) = match receipt::find(&mut tx, &attempt, &prepared).await? {
            Some(saved) if saved.settled => (saved, None),
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
        if let Some(owing) = owing {
            self.pay(&owing).await;
        }
        Ok(ack)
    }

    /// Settle a committed report's lines and release the attempt's hold.
    ///
    /// Neither call fails the report: its lines are already logged, so a biller
    /// that cannot answer leaves them unsettled for the billing sweeper, and a
    /// hold left open is released by that sweeper's pass over finished attempts.
    async fn pay(&self, owing: &settle::Owing) {
        let work_id = owing.lines.work.id().as_str();
        if let Err(fault) = self.biller.settle(&owing.lines).await {
            tracing::warn!(
                error = %fault,
                error_code = fault.code().as_str(),
                work_id,
                event = EVENT_SETTLE_DEFERRED,
                "the setup chat's usage is logged but not yet charged; the billing sweeper settles it"
            );
        }
        if let Err(fault) = self.biller.release(&owing.lines.work, owing.release).await {
            tracing::warn!(
                error = %fault,
                error_code = fault.code().as_str(),
                work_id,
                event = EVENT_RELEASE_DEFERRED,
                "the setup chat's hold is still open; the billing sweeper releases it"
            );
        }
    }
}

/// Logged when a settlement after commit did not land; the sweeper retries it.
const EVENT_SETTLE_DEFERRED: &str = "setup_chat_settle_deferred";

/// Logged when a hold release after a report did not land; the sweeper
/// retries it.
const EVENT_RELEASE_DEFERRED: &str = "setup_chat_release_deferred";

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
    ) -> Result<(Reported, Option<settle::Owing>)> {
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
