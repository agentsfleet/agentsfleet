//! Admission pins authority, capacity and bounded money in one transaction.

mod context;
mod issue;
mod selection;
mod sql;

use afd_core::id::Uuid7;
use afd_runner::capacity::choice::{self, Ticket};
use afd_wire::lease::LeaseKind;
use sqlx::Connection as _;

use crate::{Drafts, Result, error::raise::query, funnel::Owed, types::Candidate};
use selection::Hint;

/// The complete private placement returned only to the authenticated runner.
pub type Claimed = afd_wire::authoring::Lease<Candidate>;

enum Placement {
    Busy,
    Skipped,
    Admitted(Box<Claimed>),
}

impl Drafts {
    /// Admits one oldest queued message within an eligible private draft.
    ///
    /// Capacity is shared with fleet leases. A failed model or wallet gate is
    /// terminal and durable; another poll can continue with other queued work.
    ///
    /// # Errors
    /// Propagates datastore/entropy failures; ordinary ineligibility yields no work.
    pub async fn claim(&self, runner: &Uuid7) -> Result<Option<Claimed>> {
        self.select_claim(runner, None).await
    }

    /// Admit under the shared poll's optimistic host choice.
    ///
    /// # Errors
    /// Preserves admission failures; a concurrent winning poll yields no work.
    pub async fn claim_offered(&self, runner: &Uuid7, ticket: Ticket) -> Result<Option<Claimed>> {
        self.select_claim(runner, Some(ticket)).await
    }

    async fn select_claim(
        &self,
        runner: &Uuid7,
        ticket: Option<Ticket>,
    ) -> Result<Option<Claimed>> {
        for hint in selection::candidates(&self.database).await? {
            match self.admit(runner, &hint, ticket).await? {
                Placement::Admitted(lease) => return Ok(Some(*lease)),
                Placement::Busy => return Ok(None),
                Placement::Skipped => {}
            }
        }
        Ok(None)
    }

    /// Runs the production admission transaction for one fixture-owned draft.
    ///
    /// # Errors
    /// Preserves the same refusals and datastore failures as `claim`.
    #[cfg(feature = "test-util")]
    pub async fn claim_queued(&self, runner: &Uuid7, draft: &Uuid7) -> Result<Option<Claimed>> {
        let hint = selection::hint(&mut *self.database.acquire().await?, draft).await?;
        Ok(match self.admit(runner, &hint, None).await? {
            Placement::Admitted(lease) => Some(*lease),
            Placement::Busy | Placement::Skipped => None,
        })
    }

    async fn admit(
        &self,
        runner: &Uuid7,
        hint: &Hint,
        ticket: Option<Ticket>,
    ) -> Result<Placement> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(sql::CONTEXT))?;
        let Some(host) = afd_runner::capacity::lock(&mut tx, runner).await? else {
            return Ok(Placement::Busy);
        };
        if !host.can_claim()
            || ticket.is_some_and(|ticket| !ticket.permits(host, LeaseKind::Authoring))
        {
            return Ok(Placement::Busy);
        }
        let mut owed = Owed::default();
        let Some(selected) = selection::authorize(&mut tx, hint, &mut owed).await? else {
            tx.commit().await.map_err(query(sql::CONTEXT))?;
            owed.settle(&self.analytics);
            return Ok(Placement::Skipped);
        };
        let lease = issue::admit(self, &mut tx, runner, hint, selected, &mut owed).await?;
        if lease.is_some() && !choice::record(&mut tx, runner, host, LeaseKind::Authoring).await? {
            return Ok(Placement::Busy);
        }
        tx.commit().await.map_err(query(sql::CONTEXT))?;
        owed.settle(&self.analytics);
        Ok(lease.map_or(Placement::Skipped, |lease| {
            Placement::Admitted(Box::new(lease))
        }))
    }
}
