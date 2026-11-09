//! Report replay outlives private text; a settlement's usage lines commit with it,
//! and the billing relay moves its money from those lines.

mod prepare;
mod receipt;
mod retained;
mod settle;
mod sql;
mod usage;
pub(crate) mod verdict;

use af_core::id::Uuid7;
use af_wire::authoring::{Outcome, Report, Reported};
use sqlx::{PgConnection, Transaction};

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
        self.report_with_save(runner, lease, request, None).await
    }

    pub(crate) async fn report_with_save(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        request: Report,
        creations: Option<&crate::Creations>,
    ) -> Result<Reported> {
        let prepared = prepare::Prepared::new(request)?;
        let mut tx = Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(query(CONTEXT))?;
        let mut owed = Owed::default();
        let private =
            prepare::publication(self, &mut tx, runner, lease, &prepared, &mut owed).await?;
        let attempt = self
            .accounting(&mut tx, runner, lease, &prepared.counters)
            .await?;
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
        let mut announcement = afd_events::activity::Announcement::default();
        if !ack.replayed
            && ack.state == Outcome::Completed
            && let (Some(creations), Some(private)) = (creations, private.as_ref())
        {
            // Keep optional setup writes out of the accounting future's inline size.
            announcement.append(Box::pin(creations.save_turn_on(&mut tx, private)).await?);
        }
        let streams = creations.map(crate::Creations::activity_streams);
        afd_events::activity::commit(tx, announcement, streams.as_ref())
            .await
            .map_err(query(CONTEXT))?;
        owed.settle(&self.analytics);
        if let Some(creations) = creations {
            Box::pin(creations.finish_saved_turn(&attempt)).await?;
        }
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
