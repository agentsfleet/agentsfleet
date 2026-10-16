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
    types::{InitialStart, SaveFleet, Start},
};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_db::constraint::violates_unique;
use sqlx::{Connection as _, PgConnection};

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
        now: UnixMillis,
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
        now: UnixMillis,
    ) -> Result<Uuid7> {
        let mut connection = self.setup.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let scope = format!("{}:fleet:create:{}", access.workspace, person.id);
        let digest = replay::digest(&**request)?;
        if let Some(id) = replay::find(&mut tx, &scope, operation, &person.id, &digest).await? {
            tx.commit().await.map_err(query(CONTEXT))?;
            return Ok(id);
        }
        let mut captured = self
            .capture(&mut tx, access, &person, &request.source)
            .await?;
        // Waiting on the private source can let an identical Save commit.
        // Its receipt wins before the new lifecycle state is interpreted.
        if let Some(id) = replay::find(&mut tx, &scope, operation, &person.id, &digest).await? {
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
                now,
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
            replay::find(&mut tx, &scope, operation, &person.id, &digest)
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
        now: UnixMillis,
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
            now.as_millis(),
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
        now: UnixMillis,
    ) -> Result<()> {
        insert_fleet(connection, id, captured, &instance, checked, now).await?;
        self.insert_revision(
            connection,
            id,
            captured,
            checked.effective.runtime_model_ref.as_deref(),
            now,
        )
        .await?;
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
                .bind("saving")
                .bind(now.as_millis())
                .execute(connection)
                .await
                .map_err(query(CONTEXT))?;
        }
        Ok(())
    }

    async fn insert_revision(
        &self,
        connection: &mut PgConnection,
        fleet: &Uuid7,
        captured: &Captured,
        model: Option<&str>,
        now: UnixMillis,
    ) -> Result<()> {
        let revision = self.setup.drafts.mint(now)?;
        sqlx::query(sql::REVISION)
            .bind(revision.as_str())
            .bind(fleet.as_str())
            .bind(INITIAL_VERSION)
            .bind(serde_json::to_string(&captured.candidate.definition).map_err(stored)?)
            .bind(&captured.candidate.canonical_digest)
            .bind(&captured.candidate.policy_semantics)
            .bind(captured.library_revision.as_ref().map(Uuid7::as_str))
            .bind(now.as_millis())
            .bind(model)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
        sqlx::query(sql::SELECT_REVISION)
            .bind(fleet.as_str())
            .bind(revision.as_str())
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(())
    }
}

/// The fleet row itself, `INSTALLING` under the accepted review. A name another
/// fleet in the workspace already holds answers `NameExists`, not a query error.
async fn insert_fleet(
    connection: &mut PgConnection,
    id: &Uuid7,
    captured: &Captured,
    instance: &Instance<'_>,
    checked: &crate::types::Review,
    now: UnixMillis,
) -> Result<()> {
    let definition = &captured.candidate.definition;
    let accepted = Accepted {
        context: captured.context.clone(),
        candidate_id: captured.candidate.id.clone(),
        candidate_revision: captured.candidate.revision.clone(),
        canonical_digest: captured.candidate.canonical_digest.clone(),
        policy_semantics: captured.candidate.policy_semantics.clone(),
        review_digest: checked.review_digest.clone(),
    };
    sqlx::query(sql::INSERT)
        .bind(id.as_str())
        .bind(captured.context.workspace_id.as_str())
        .bind(captured.context.tenant_id.as_str())
        .bind(instance.name)
        .bind(serde_json::to_string(definition.config()).map_err(stored)?)
        .bind(INSTALLING)
        .bind(now.as_millis())
        .bind(INITIAL_VERSION)
        .bind(serde_json::to_string(&accepted).map_err(stored)?)
        .bind(serde_json::to_string(&InitialStart::new(instance.start.clone())).map_err(stored)?)
        .bind(&checked.effective.runtime_model_ref)
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
