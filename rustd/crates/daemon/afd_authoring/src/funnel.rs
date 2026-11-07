//! The private-chat-to-fleet funnel steps this crate reports.
//!
//! # Reported after the commit, never before
//!
//! A step is a durable fact, so it is reported only once the transaction that
//! made it true has committed: a rollback after the write reports nothing.
//! Writers several calls deep hand their step to an [`Owed`] the transaction's
//! owner holds, and the owner settles it after `commit` returns.
//!
//! # Once per real transition
//!
//! Each step is earned from a guarded write — a fresh replay reservation, an
//! update that matched a non-terminal row — so a replay, a refusal or a lost
//! race matches nothing and owes nothing.
//!
//! # Reporting cannot fail the flow
//!
//! [`Analytics::report`] queues and returns; it has no error to raise. The one
//! datastore read here, a creator's subject, is a primary-key read of the
//! users row every authoring guard already reads; a missing person drops the
//! step rather than refusing the write.

use afd_core::id::Uuid7;
use afd_observability::{Analytics, AuthoringStep, Telemetry};
use sqlx::PgConnection;

use crate::{Access, Drafts, Result, error::raise::query, lease::guard::Attempt};

const CONTEXT: &str = "attribute a private-chat funnel step";

/// A first publication creates the Library entry.
pub(crate) const PUBLISHED_ENTRY: &str = "entry";

/// A later publication appends a revision to an entry.
pub(crate) const PUBLISHED_REVISION: &str = "revision";

impl Drafts {
    /// Reports `step`, attributed to the person acting in `access`. Call only
    /// after the transaction that made it true committed.
    pub(crate) fn reached(&self, access: Access<'_>, step: AuthoringStep) {
        self.analytics.report(&Telemetry::Authoring {
            actor: access.subject.to_owned(),
            workspace_id: access.workspace.clone(),
            step,
        });
    }
}

/// The private message a terminal transition happened to.
#[derive(Clone, Copy)]
pub(crate) struct Message<'a> {
    pub(crate) workspace: &'a Uuid7,
    pub(crate) creator: &'a Uuid7,
    pub(crate) draft: &'a Uuid7,
}

impl<'a> From<&'a Attempt> for Message<'a> {
    fn from(attempt: &'a Attempt) -> Self {
        Self {
            workspace: &attempt.workspace_id,
            creator: &attempt.user_id,
            draft: &attempt.draft_id,
        }
    }
}

/// Steps a transaction has earned, held until it commits.
#[derive(Debug, Default)]
#[must_use = "owed steps are reported only by `settle`, after the commit"]
pub(crate) struct Owed(Vec<Telemetry>);

impl Owed {
    /// Owes `message`'s finish in `state`, attributed to its creator.
    ///
    /// # Errors
    /// Propagates a datastore failure the surrounding transaction would also hit.
    pub(crate) async fn finished(
        &mut self,
        connection: &mut PgConnection,
        message: Message<'_>,
        state: &'static str,
    ) -> Result<()> {
        let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
            .bind(message.creator.as_str())
            .fetch_optional(connection)
            .await
            .map_err(query(CONTEXT))?;
        if let Some(actor) = subject {
            self.finished_by(actor, message, state);
        }
        Ok(())
    }

    /// Owes `message`'s finish, for a caller that already holds the subject.
    pub(crate) fn finished_by(&mut self, actor: String, message: Message<'_>, state: &'static str) {
        self.0.push(Telemetry::Authoring {
            actor,
            workspace_id: message.workspace.clone(),
            step: AuthoringStep::MessageFinished {
                draft_id: message.draft.clone(),
                state,
            },
        });
    }

    /// Reports every owed step. Call only after the transaction committed.
    pub(crate) fn settle(self, analytics: &Analytics) {
        for owed in &self.0 {
            analytics.report(owed);
        }
    }
}
