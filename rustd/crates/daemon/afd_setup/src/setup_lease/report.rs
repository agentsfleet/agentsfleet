//! The draft's half of a setup lease's report: one candidate at most, the
//! message's finish, and any save the message asked for, in the report's
//! transaction.
//!
//! The runner applied every `propose_setup` call in memory and reports the
//! final Definition on the base it started from. The daemon re-checks that
//! base against the draft's head once, under the draft's lock, and parses the
//! Definition through the one parser every candidate passes. A base that moved
//! or a Definition that does not parse writes no candidate; the answer stands.
//! A replayed report reaches none of this: the lease's settlement, which runs
//! first, finds the lease already settled.

use af_core::{error_code, id::Uuid7};
use af_fleet_runtime::Definition;
use af_wire::{authoring::FailureReason, lease::SetupProposal};
use afd_turns::view::activity::Announcement;
use jiff::Timestamp;
use sqlx::PgConnection;

use super::{SetupOwed, sql};
use crate::{
    Access, Creations, Result, access, candidate,
    creation::chat_save::{SaveTurn, SavedKey},
    draft::read,
    error::raise::query,
    funnel::Message,
    lease::report::{sql::MESSAGE, verdict},
    types::{CANCELLED, MessageState, RUNNING},
};

/// Logged when a report's setup was not written: its base moved or it did
/// not parse.
const EVENT_NOT_APPLIED: &str = "setup_report_not_applied";

/// [`sql::LOCK_REPORT`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct LockedRow {
    workspace_id: Uuid7,
    tenant_id: Uuid7,
    user_id: Uuid7,
    sequence: i64,
    status: String,
    save_requested: bool,
    oidc_subject: String,
}

/// The draft a setup report finishes, locked in the report's transaction.
/// Its `Debug` names the draft only: the rest is private conversation state.
pub struct SetupReportScope {
    draft_id: Uuid7,
    row: LockedRow,
    /// The draft as its creator holds it; `None` once they lost the workspace.
    private: Option<read::Locked>,
}

impl std::fmt::Debug for SetupReportScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupReportScope")
            .field("draft_id", &self.draft_id)
            .finish_non_exhaustive()
    }
}

impl SetupReportScope {
    /// Which save request [`Creations::after_setup_report`] finishes.
    fn key(&self) -> SavedKey {
        SavedKey {
            draft_id: self.draft_id.clone(),
            workspace_id: self.row.workspace_id.clone(),
            user_id: self.row.user_id.clone(),
            tenant_id: self.row.tenant_id.clone(),
            sequence: self.row.sequence,
        }
    }

    const fn message(&self) -> Message<'_> {
        Message {
            workspace: &self.row.workspace_id,
            creator: &self.row.user_id,
            draft: &self.draft_id,
        }
    }
}

/// What the runner reported for a setup turn.
#[derive(Debug, Clone, Copy)]
pub struct SetupFinish<'a> {
    /// The turn ran to an answer.
    pub completed: bool,
    /// The answer, when it gave one.
    pub answer: Option<&'a str>,
    /// The final setup the turn proposed, on the base it started from.
    pub proposal: Option<&'a SetupProposal<'a>>,
    /// The report's clock.
    pub now: Timestamp,
}

impl Creations {
    /// Locks `draft`, the owner a setup lease's report settles, and reads the
    /// message `message` it ran. Run first in the report's transaction, before
    /// the lease and its slot are locked. `None` once the draft is deleted:
    /// the report then settles the lease and its usage and writes nothing else.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn lock_setup_report(
        &self,
        connection: &mut PgConnection,
        draft: &Uuid7,
        message: &Uuid7,
    ) -> Result<Option<SetupReportScope>> {
        let row: Option<LockedRow> = sqlx::query_as(sql::LOCK_REPORT)
            .bind(draft)
            .bind(message)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(sql::CONTEXT))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let access = Access {
            workspace: &row.workspace_id,
            subject: &row.oidc_subject,
        };
        let private = match access::current(&mut *connection, access).await {
            Ok(person) if person.id == row.user_id => {
                Some(read::lock(&mut *connection, access, &person, draft).await?)
            }
            Ok(_) => None,
            Err(error) if error.code() == error_code::AUTHORING_NOT_FOUND => None,
            Err(error) => return Err(error),
        };
        Ok(Some(SetupReportScope {
            draft_id: draft.clone(),
            row,
            private,
        }))
    }

    /// Finishes the message `scope` names, once the lease's settlement in the
    /// same transaction succeeded: one candidate when the proposal still
    /// stands on the draft's head, the answer, and any save it asked for.
    ///
    /// A message no longer running (a Stop finished it) is left as it is. A
    /// creator who lost the workspace gets no private write: the message is
    /// cancelled, as a revoked creator's queued work is.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn finish_setup_turn(
        &self,
        connection: &mut PgConnection,
        scope: &SetupReportScope,
        finish: SetupFinish<'_>,
        owed: &mut SetupOwed,
    ) -> Result<Announcement> {
        if scope.row.status != RUNNING {
            return Ok(Announcement::default());
        }
        let Some(draft) = &scope.private else {
            write_message(connection, scope, CANCELLED, None, None, None).await?;
            owed.0
                .finished(connection, scope.message(), CANCELLED)
                .await?;
            return Ok(Announcement::default());
        };
        if !finish.completed {
            let failed = MessageState::Failed.as_str();
            let reason = verdict::reason(FailureReason::ProviderFailed);
            let head = draft.head_candidate_id.as_ref();
            write_message(connection, scope, failed, None, head, Some(reason)).await?;
            owed.0.finished(connection, scope.message(), failed).await?;
            return Ok(Announcement::default());
        }
        // A save turn proposes nothing: it saves the head the person reviewed.
        let written = match finish.proposal {
            Some(proposal) if !scope.row.save_requested => {
                self.apply_setup(connection, draft, proposal, finish.now)
                    .await?
            }
            Some(_) | None => None,
        };
        let candidate = written.as_ref().or(draft.head_candidate_id.as_ref());
        let completed = MessageState::Completed.as_str();
        write_message(connection, scope, completed, finish.answer, candidate, None).await?;
        owed.0
            .finished(connection, scope.message(), completed)
            .await?;
        if !scope.row.save_requested {
            return Ok(Announcement::default());
        }
        let turn = SaveTurn {
            draft,
            sequence: scope.row.sequence,
            now: finish.now,
        };
        Box::pin(self.save_turn_on(connection, turn)).await
    }

    /// Commits a setup report's transaction, then announces what its save
    /// changed, as the attempt path's report does.
    ///
    /// # Errors
    /// Reports a commit the datastore refused.
    pub async fn commit_setup_report(
        &self,
        transaction: sqlx::Transaction<'static, sqlx::Postgres>,
        announcement: Announcement,
        owed: SetupOwed,
    ) -> Result<()> {
        let streams = self.activity_streams();
        afd_turns::view::activity::commit(transaction, announcement, Some(&streams))
            .await
            .map_err(query(sql::CONTEXT))?;
        owed.settle(&self.setup.drafts);
        Ok(())
    }

    /// After the report committed: retries a requested save's activation and
    /// registers its triggers, as the attempt path's report does.
    ///
    /// # Errors
    /// Preserves activation and schedule failures.
    pub async fn after_setup_report(&self, scope: &SetupReportScope) -> Result<()> {
        if !scope.row.save_requested || scope.private.is_none() {
            return Ok(());
        }
        Box::pin(self.finish_saved_turn(&scope.key())).await
    }

    /// Re-checks `proposal`'s base against the draft's head and parses its
    /// Definition, and appends it as the next candidate: its id, or `None`
    /// when the base moved or the Definition does not parse.
    async fn apply_setup(
        &self,
        connection: &mut PgConnection,
        draft: &read::Locked,
        proposal: &SetupProposal<'_>,
        now: Timestamp,
    ) -> Result<Option<Uuid7>> {
        let head = draft.head_candidate_id.as_ref().map(Uuid7::as_str);
        let base = proposal.base_candidate_id.as_deref();
        let revision = u64::try_from(draft.candidate_revision).unwrap_or(u64::MAX);
        if head != base || revision != proposal.base_revision {
            not_applied(
                &draft.id,
                "the base candidate is no longer the draft's head",
            );
            return Ok(None);
        }
        let definition = match Definition::parse(&proposal.definition) {
            Ok(definition)
                if head.is_some() || definition.validate_initial_generation().is_ok() =>
            {
                definition
            }
            Ok(_) | Err(_) => {
                not_applied(&draft.id, "the reported setup does not parse");
                return Ok(None);
            }
        };
        let id = self.setup.drafts.mint(now)?;
        candidate::append(
            connection,
            candidate::Replacement {
                id: &id,
                draft: &draft.id,
                revision: draft.candidate_revision.saturating_add(1),
                definition: &definition,
            },
        )
        .await?;
        Ok(Some(id))
    }
}

/// Writes the running message's finish; a message a Stop finished matches
/// nothing.
async fn write_message(
    connection: &mut PgConnection,
    scope: &SetupReportScope,
    status: &str,
    answer: Option<&str>,
    candidate: Option<&Uuid7>,
    reason: Option<&str>,
) -> Result<()> {
    sqlx::query(MESSAGE)
        .bind(&scope.draft_id)
        .bind(scope.row.sequence)
        .bind(status)
        .bind(answer)
        .bind(candidate)
        .bind(reason)
        .bind(RUNNING)
        .execute(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(())
}

fn not_applied(draft: &Uuid7, why: &'static str) {
    let draft_id = draft.as_str();
    {
        let (event, message) = (
            &EVENT_NOT_APPLIED,
            format_args!("{why}; no candidate written"),
        );
        tracing::warn!(message, draft_id, event);
    };
}
