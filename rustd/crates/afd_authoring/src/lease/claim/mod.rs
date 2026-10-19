//! Admission pins authority, capacity and bounded money in one transaction.

mod context;
mod issue;
mod selection;
mod sql;

use afd_billing::biller::{Hold, Reservation};
use afd_core::id::Uuid7;
use afd_runner::capacity::choice::{self, Ticket};
use afd_wire::lease::LeaseKind;
use sqlx::Connection as _;

use crate::{Drafts, Result, error::raise::query, funnel::Owed, types::Candidate};
use selection::Hint;

/// The complete private placement returned only to the authenticated runner.
pub type Claimed = afd_wire::authoring::Lease<Candidate>;

/// Logged when a freshly claimed attempt's hold was not placed.
const EVENT_HOLD_UNPLACED: &str = "setup_chat_hold_unplaced";

/// What [`EVENT_HOLD_UNPLACED`] says, whichever way the hold was missed.
const MESSAGE_HOLD_UNPLACED: &str =
    "the setup chat runs unheld; its first model call reserves again";

enum Placement {
    Busy,
    Skipped,
    Admitted(Box<Claimed>),
}

impl Drafts {
    /// Admits one oldest queued message within an eligible private draft, under
    /// the shared poll's optimistic host choice.
    ///
    /// Capacity is shared with fleet leases. A failed model or wallet gate is
    /// terminal and durable; another poll can continue with other queued work.
    ///
    /// # Errors
    /// Preserves admission failures; a concurrent winning poll yields no work.
    pub async fn claim_offered(&self, runner: &Uuid7, ticket: Ticket) -> Result<Option<Claimed>> {
        for hint in selection::candidates(&self.database).await? {
            match self.admit(runner, &hint, Some(ticket)).await? {
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
    /// Preserves the same refusals and datastore failures as `claim_offered`.
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
        let admitted = issue::admit(self, &mut tx, runner, hint, selected, &mut owed).await?;
        if admitted.is_some()
            && !choice::record(&mut tx, runner, host, LeaseKind::Authoring).await?
        {
            return Ok(Placement::Busy);
        }
        tx.commit().await.map_err(query(sql::CONTEXT))?;
        owed.settle(&self.analytics);
        let Some((lease, hold)) = admitted else {
            return Ok(Placement::Skipped);
        };
        self.hold_claimed(&hold).await;
        Ok(Placement::Admitted(Box::new(lease)))
    }

    /// Place a freshly claimed attempt's hold: the bound its gate admitted.
    ///
    /// After the attempt commits, because the hold names the attempt and a
    /// biller can never join our transaction. A balance that moved since the
    /// gate, or a biller that could not answer, leaves the attempt unheld; its
    /// first model call raises the hold and refuses there if the balance cannot
    /// fund it.
    async fn hold_claimed(&self, hold: &Reservation) {
        let work_id = hold.work.id().as_str();
        match self.biller.reserve(hold).await {
            Ok(Hold::Held { .. } | Hold::Unconfirmed) => {}
            Ok(answer) => tracing::warn!(
                work_id,
                hold = ?answer,
                event = EVENT_HOLD_UNPLACED,
                MESSAGE_HOLD_UNPLACED
            ),
            Err(fault) => {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = fault.code().as_str();
                tracing::warn!(
                    error = %fault,
                    error_code,
                    work_id,
                    event = EVENT_HOLD_UNPLACED,
                    MESSAGE_HOLD_UNPLACED
                );
            }
        }
    }
}
