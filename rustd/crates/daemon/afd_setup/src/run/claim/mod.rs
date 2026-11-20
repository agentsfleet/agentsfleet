//! Admission pins authority, capacity and bounded money in one transaction.

mod context;
mod issue;
pub(crate) mod selection;
mod sql;

use af_core::id::Uuid7;
use af_wire::run::RunKind;
use afd_runner::capacity::choice::{self, Ticket};
use sqlx::Connection as _;

use crate::{Drafts, Result, error::raise::query, funnel::Owed, types::Candidate};
use selection::{Authorized, Hint};

/// The complete private placement returned only to the authenticated runner.
pub type Claimed = af_wire::authoring::Run<Candidate>;

/// Logged when a claim skips a candidate whose creator or draft another
/// transaction holds.
const EVENT_DRAFT_SKIPPED: &str = "authoring_claim_draft_skipped";

/// What [`EVENT_DRAFT_SKIPPED`] says.
const MESSAGE_DRAFT_SKIPPED: &str =
    "a held draft was skipped, not waited on; a later poll admits it";

enum Placement {
    Busy,
    Skipped,
    Admitted(Box<Claimed>),
}

impl Drafts {
    /// The compiled discovery statement, for real database query-plan measurements.
    #[cfg(feature = "test-util")]
    pub const QUEUED_DRAFTS_QUERY: &'static str = sql::CANDIDATES;

    /// Admits one oldest queued message within an eligible private draft, under
    /// the shared poll's optimistic host choice.
    ///
    /// Capacity is shared with fleet runs. A failed model or wallet gate is
    /// terminal and durable; another poll can continue with other queued work.
    ///
    /// # Errors
    /// Preserves admission failures; a concurrent winning poll yields no work.
    pub async fn claim_assigned(&self, runner: &Uuid7, ticket: Ticket) -> Result<Option<Claimed>> {
        for hint in selection::candidates(&self.database, runner).await? {
            match self.admit(runner, &hint, Some(ticket)).await? {
                Placement::Admitted(run) => return Ok(Some(*run)),
                Placement::Busy => return Ok(None),
                Placement::Skipped => {}
            }
        }
        Ok(None)
    }

    /// Runs the production admission transaction for one fixture-owned draft.
    ///
    /// # Errors
    /// Preserves the same refusals and datastore failures as `claim_assigned`.
    #[cfg(feature = "test-util")]
    pub async fn claim_queued(&self, runner: &Uuid7, draft: &Uuid7) -> Result<Option<Claimed>> {
        let hint = selection::hint(&mut *self.database.acquire().await?, draft).await?;
        Ok(match self.admit(runner, &hint, None).await? {
            Placement::Admitted(run) => Some(*run),
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
        let Some(host) = self
            .lock_host_capacity(
                &mut tx,
                runner,
                afd_runner::capacity::Claimant::Authoring,
                Some(&hint.tenant_id),
            )
            .await?
        else {
            return Ok(Placement::Busy);
        };
        if !host.can_claim()
            || ticket.is_some_and(|ticket| !ticket.permits(host, RunKind::Authoring))
        {
            return Ok(Placement::Busy);
        }
        // Before any write: a foreign runner leaves no attempt and no failure behind.
        if !host.serves() {
            afd_runner::capacity::refused_placement(
                runner,
                afd_runner::capacity::PlacementRefusal::ForeignTenant,
                RunKind::Authoring.as_str(),
            );
            return Ok(Placement::Skipped);
        }
        let mut owed = Owed::default();
        let selected = match selection::authorize(&mut tx, hint, &mut owed).await? {
            Authorized::Selected(selected) => *selected,
            passed @ (Authorized::Passed | Authorized::Locked) => {
                tx.commit().await.map_err(query(sql::CONTEXT))?;
                owed.settle(&self.analytics);
                if matches!(passed, Authorized::Locked) {
                    // Debug: under contention this fires on every poll of the walk.
                    let runner_id = runner.as_str();
                    {
                        let event = &EVENT_DRAFT_SKIPPED;
                        tracing::debug!(runner_id, event, MESSAGE_DRAFT_SKIPPED);
                    };
                }
                return Ok(Placement::Skipped);
            }
        };
        let admitted = issue::admit(self, &mut tx, runner, hint, selected, &mut owed).await?;
        if admitted.is_some() && !choice::record(&mut tx, runner, host, RunKind::Authoring).await? {
            return Ok(Placement::Busy);
        }
        tx.commit().await.map_err(query(sql::CONTEXT))?;
        owed.settle(&self.analytics);
        let Some(run) = admitted else {
            return Ok(Placement::Skipped);
        };
        Ok(Placement::Admitted(Box::new(run)))
    }
}
