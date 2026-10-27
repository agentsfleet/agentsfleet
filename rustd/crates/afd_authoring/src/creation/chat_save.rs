//! A completed save request and its immutable setup share the report transaction.

use super::{CONTEXT, Creations, KILLED, read, reserve::Instance, revision, source::Captured};
use crate::{
    Access, Result, access, candidate, draft,
    error::{ErrorKind, raise::query},
    lease::guard::{Attempt, Guarded},
    message, review,
    types::{DraftState, MessageState, SourcePin, Start},
};
use afd_core::{id::Uuid7, problem::Problem};
use afd_events::activity::Announcement;
use afd_wire::authoring::{Report, Reported};
use sqlx::{Connection as _, PgConnection};

impl Creations {
    /// Completes a setup turn and any save that its accepted message requested.
    ///
    /// A checked save refusal is returned in message history. The answer remains
    /// available. Successful saves survive a closed browser and report retries.
    ///
    /// # Errors
    /// Refuses invalid fences or report replay and propagates datastore outages.
    pub async fn report(&self, runner: &Uuid7, lease: &Uuid7, request: Report) -> Result<Reported> {
        self.setup
            .drafts
            .report_with_save(runner, lease, request, Some(self))
            .await
    }

    pub(crate) async fn save_turn_on(
        &self,
        connection: &mut PgConnection,
        guarded: &Guarded,
    ) -> Result<Announcement> {
        let attempt = &guarded.attempt;
        if !message::save_requested(connection, &attempt.draft_id, attempt.message_sequence).await?
        {
            return Ok(Announcement::default());
        }
        let mut savepoint = connection.begin().await.map_err(query(CONTEXT))?;
        match self.write_turn(&mut savepoint, guarded).await {
            Ok(announcement) => {
                savepoint.commit().await.map_err(query(CONTEXT))?;
                Ok(announcement)
            }
            Err(error) => {
                savepoint.rollback().await.map_err(query(CONTEXT))?;
                if Problem::of(error.code()).status() >= 500 {
                    return Err(error);
                }
                sqlx::query(include_str!("sql/save_refused.sql"))
                    .bind(attempt.draft_id.as_str())
                    .bind(attempt.message_sequence)
                    .bind(error.code().as_str())
                    .execute(connection)
                    .await
                    .map_err(query(CONTEXT))?;
                Ok(Announcement::default())
            }
        }
    }

    async fn write_turn(
        &self,
        connection: &mut PgConnection,
        guarded: &Guarded,
    ) -> Result<Announcement> {
        let locked = &guarded.draft;
        draft::idle(connection, locked).await?;
        let id = locked.head_candidate_id.as_ref().ok_or(ErrorKind::State {
            current: "no_candidate",
        })?;
        let candidate = candidate::read(connection, &locked.id, id).await?;
        candidate.definition.validate_for_save()?;
        if candidate.policy_semantics != afd_fleet_runtime::definition::POLICY_SEMANTICS {
            return Err(ErrorKind::Policy.into());
        }
        let mut captured = Captured {
            candidate,
            context: review::Context::from(locked),
            draft: Some(locked.clone()),
        };
        let name = locked.instance_name.clone().unwrap_or_else(|| {
            captured
                .candidate
                .definition
                .runtime()
                .name()
                .as_str()
                .to_owned()
        });
        afd_fleet_runtime::FleetName::parse(&name)?;
        captured.context.instance_name = Some(name.clone());
        let checked = review::build(
            &self.setup,
            connection,
            &captured.context,
            &captured.candidate,
            guarded.now,
        )
        .await?;
        if let Some(SourcePin::Fleet { fleet_id, .. }) = &locked.source_pin {
            let row = read::lock(connection, &locked.workspace_id, fleet_id).await?;
            revision::target(&captured, fleet_id, row.head_revision)?;
            if row.status == KILLED {
                return Err(ErrorKind::State { current: "killed" }.into());
            }
            self.write_improvement(
                connection,
                &row,
                &captured,
                &checked,
                &self.setup.drafts.mint(guarded.now)?,
                guarded.now,
            )
            .await
        } else {
            let instance = Instance {
                name: &name,
                start: &Start::None,
            };
            self.insert(
                connection,
                &self.setup.drafts.mint(guarded.now)?,
                &captured,
                instance,
                &checked,
                guarded.now,
            )
            .await
        }
    }

    /// Retries activation after a committed report, including after a lost acknowledgement.
    pub(crate) async fn finish_saved_turn(&self, attempt: &Attempt) -> Result<()> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let fleet: Option<String> = sqlx::query_scalar(include_str!("sql/saved_turn.sql"))
            .bind(attempt.draft_id.as_str())
            .bind(attempt.workspace_id.as_str())
            .bind(attempt.user_id.as_str())
            .bind(attempt.message_sequence)
            .bind(MessageState::Completed.as_str())
            .bind(DraftState::Saving.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        let Some(fleet) = fleet else {
            return Ok(());
        };
        let subject: Option<String> = sqlx::query_scalar(crate::sql::access::SUBJECT)
            .bind(attempt.user_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        let Some(subject) = subject else {
            return Ok(());
        };
        let access = Access {
            workspace: &attempt.workspace_id,
            subject: &subject,
        };
        match access::current(&mut connection, access).await {
            Ok(person) if person.id == attempt.user_id && person.owner == attempt.tenant_id => {}
            Ok(_) => return Ok(()),
            Err(error) if error.code() == afd_core::error_code::AUTHORING_NOT_FOUND => {
                return Ok(());
            }
            Err(error) => return Err(error),
        }
        drop(connection);
        self.advance(access, &Uuid7::parse(&fleet)?, afd_core::clock::now())
            .await
    }
}
