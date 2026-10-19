//! The creation key and fleet row commit before any queue or approval operation.

use super::{
    CONTEXT, Creations, INITIAL_VERSION, INSTALLING, SavedFleet, source::Captured, sql,
    stored::Accepted,
};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    replay, review,
    types::{DraftState, InitialStart, SaveFleet, Start},
};
use afd_core::id::Uuid7;
use afd_db::constraint::violates_unique;
use jiff::Timestamp;
use sqlx::{Connection as _, PgConnection};

/// Capability tags a new fleet needs before any runner may place it: none.
const NO_TAGS: &[&str] = &[];

/// The instance a new fleet row is written as: its slug and initial start.
pub(super) struct Instance<'a> {
    pub(super) name: &'a str,
    pub(super) start: &'a Start,
}

impl Creations {
    /// Saves one reviewed source; an exact retry resumes the accepted fleet.
    ///
    /// # Errors
    /// Refuses changed keys, private sources, active messages and stale review
    /// before allocation. After allocation, failures retain the same identity.
    pub async fn save(
        &self,
        access: Access<'_>,
        operation: &Uuid7,
        request: SaveFleet,
        now: Timestamp,
    ) -> Result<SavedFleet> {
        let request = garde::Unvalidated::new(request).validate()?;
        afd_fleet_runtime::FleetName::parse(&request.name)?;
        let id = self.reserve(access, operation, &request, now).await?;
        self.advance(access, &id, now).await?;
        self.read(access, &id).await
    }

    async fn reserve(
        &self,
        access: Access<'_>,
        operation: &Uuid7,
        request: &garde::Valid<SaveFleet>,
        now: Timestamp,
    ) -> Result<Uuid7> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:fleet:create:{}", access.workspace, person.id);
        let digest = replay::digest(&**request)?;
        if let Some(id) = replay::find(
            &mut tx,
            &scope,
            replay::Resource::Fleet,
            operation,
            &person.id,
            &digest,
        )
        .await?
        {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(id);
        }
        let mut captured = self
            .capture(&mut tx, access, &person, &request.source)
            .await?;
        // Waiting on the private source can let an identical Save commit.
        // Its receipt wins before the new lifecycle state is interpreted.
        if let Some(id) = replay::find(
            &mut tx,
            &scope,
            replay::Resource::Fleet,
            operation,
            &person.id,
            &digest,
        )
        .await?
        {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(id);
        }
        let checked = self.admit(&mut tx, &mut captured, request, now).await?;
        let id = self.setup.drafts.mint(now)?;
        let won = replay::reserve(
            &mut tx,
            replay::Reservation {
                receipt_id: &self.setup.drafts.mint(now)?,
                workspace: access.workspace,
                person: &person,
                scope: &scope,
                operation,
                digest: &digest,
                resource: &id,
            },
        )
        .await?;
        let id = if won {
            let instance = Instance {
                name: &request.name,
                start: &request.start,
            };
            self.insert(&mut tx, &id, &captured, instance, &checked, now)
                .await?;
            id
        } else {
            replay::find(
                &mut tx,
                &scope,
                replay::Resource::Fleet,
                operation,
                &person.id,
                &digest,
            )
            .await?
            .ok_or(ErrorKind::Gone)?
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(id)
    }

    /// Proves the captured source may become a fleet under `request`'s name:
    /// its draft idle, its definition saveable, its review the one the client
    /// saw, and its policy semantics the ones this daemon enforces.
    async fn admit(
        &self,
        connection: &mut PgConnection,
        captured: &mut Captured,
        request: &garde::Valid<SaveFleet>,
        now: Timestamp,
    ) -> Result<crate::types::Review> {
        if let Some(draft) = &captured.draft {
            crate::draft::idle(connection, draft).await?;
        }
        captured.candidate.definition.validate_for_save()?;
        captured.context.instance_name = Some(request.name.clone());
        let checked = review::build(
            &self.setup,
            connection,
            &captured.context,
            &captured.candidate,
            now,
        )
        .await?;
        checked.require_digest(&request.review_digest)?;
        if captured.candidate.policy_semantics != afd_fleet_runtime::definition::POLICY_SEMANTICS {
            return Err(ErrorKind::Policy.into());
        }
        Ok(checked)
    }

    /// Writes the fleet row, its first immutable revision and inherited schedules.
    pub(super) async fn insert(
        &self,
        connection: &mut PgConnection,
        id: &Uuid7,
        captured: &Captured,
        instance: Instance<'_>,
        checked: &crate::types::Review,
        now: Timestamp,
    ) -> Result<()> {
        let revision = self.setup.drafts.mint(now)?;
        insert_fleet(connection, id, &revision, captured, &instance).await?;
        let accepted = Accepted::from_review(captured, checked);
        let head = Head {
            id: &revision,
            number: INITIAL_VERSION,
            accepted: &accepted,
            model: checked.effective.runtime_model_ref.as_deref(),
        };
        insert_revision(connection, id, &captured.candidate.definition, head).await?;
        self.replace_schedules(
            connection,
            &captured.context.workspace_id,
            id,
            INITIAL_VERSION,
            &captured.candidate.definition,
            now,
        )
        .await?;
        if let Some(draft) = &captured.draft {
            sqlx::query(sql::LINK)
                .bind(draft.id.as_str())
                .bind(id.as_str())
                .bind(DraftState::Saving.as_str())
                .execute(connection)
                .await
                .map_err(query(CONTEXT))?;
        }
        Ok(())
    }
}

/// One immutable revision: which it is, the setup it accepted and the model
/// review settled on.
pub(super) struct Head<'a> {
    pub(super) id: &'a Uuid7,
    pub(super) number: i64,
    pub(super) accepted: &'a Accepted,
    pub(super) model: Option<&'a str>,
}

/// Writes `head` for `fleet`, with its definition.
pub(super) async fn insert_revision(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    definition: &afd_fleet_runtime::Definition,
    head: Head<'_>,
) -> Result<()> {
    sqlx::query(sql::REVISION)
        .bind(head.id.as_str())
        .bind(fleet.as_str())
        .bind(head.number)
        .bind(serde_json::to_string(definition).map_err(stored)?)
        .bind(&head.accepted.canonical_digest)
        .bind(&head.accepted.policy_semantics)
        .bind(head.model)
        .bind(&head.accepted.review_digest)
        .bind(head.accepted.frozen()?)
        .bind(head.accepted.candidate_id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

/// The fleet row itself, `INSTALLING` under the accepted review. A name another
/// fleet in the workspace already holds answers `NameExists`, not a query error.
async fn insert_fleet(
    connection: &mut PgConnection,
    id: &Uuid7,
    head: &Uuid7,
    captured: &Captured,
    instance: &Instance<'_>,
) -> Result<()> {
    sqlx::query(sql::INSERT)
        .bind(id.as_str())
        .bind(captured.context.workspace_id.as_str())
        .bind(captured.context.tenant_id.as_str())
        .bind(instance.name)
        .bind(INSTALLING)
        .bind(head.as_str())
        .bind(NO_TAGS)
        .bind(serde_json::to_string(&InitialStart::new(instance.start.clone())).map_err(stored)?)
        .bind(INITIAL_VERSION)
        .execute(&mut *connection)
        .await
        .map_err(|source| {
            if violates_unique(&source, "uq_fleets_workspace_id_name") {
                ErrorKind::NameExists.into()
            } else {
                query(CONTEXT)(source)
            }
        })?;
    Ok(())
}
