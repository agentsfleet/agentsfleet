//! A person's change to a fleet: its name, archive, model and network
//! toggles, each under the row's strong tag.
//!
//! Whether the caller may open the network is the route's to decide: only a
//! signed-in person may change `http_egress` or `package_domains`.

use af_core::id::Uuid7;
use af_fleet_runtime::{FleetName, config::HttpEgress};
use afd_api_wire::fleet::PatchFleetRequest;
use serde_json::json;
use sqlx::{Connection as _, PgConnection};

use super::{ARCHIVED, CONTEXT, Creations, etag, model, read, sql};
use crate::{
    Access, Result, access,
    error::{
        ErrorKind,
        raise::{invalid, query},
    },
    types::SavedFleet,
};

/// The actor and type a model change's history receipt carries, which the
/// app's activity row reads.
const ACTOR_MODEL_CHANGE: &str = "config_reload";
const EVENT_TYPE_MODEL_CHANGED: &str = "model_changed";

/// The row after a patch, before it is written.
struct Changed<'a> {
    status: &'a str,
    name: &'a str,
    model_ref: &'a str,
    http_egress: HttpEgress,
    package_domains: bool,
}

impl Creations {
    /// Applies `patch` to `fleet` under the tag `expected`.
    ///
    /// A model change writes one `model_changed` receipt to the fleet's
    /// history in the same commit; the next run resolves the new model. An
    /// archive marks the fleet's live runs cancelled in the same commit.
    ///
    /// # Errors
    /// An empty patch, an invalid name, a missing or stale tag, an archived
    /// fleet, a model that resolves to nothing for the workspace
    /// (`UZ-SETUP-008`), a name the workspace holds, a caller who is not a
    /// current member, and datastore failures. A refusal changes nothing.
    pub async fn patch(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        expected: Option<&str>,
        patch: &garde::Valid<PatchFleetRequest>,
    ) -> Result<SavedFleet> {
        if patch.status.is_none()
            && patch.name.is_none()
            && patch.model_ref.is_none()
            && !patch.opens_network()
        {
            return Err(invalid(
                "A name, status, model or network setting is required",
            ));
        }
        if let Some(name) = &patch.name {
            FleetName::parse(name)?;
        }
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let row = read::lock(&mut tx, access.workspace, fleet).await?;
        let expected = expected.ok_or(ErrorKind::Precondition)?;
        let current = etag(fleet, row.resource_version);
        if expected != current {
            return Err(ErrorKind::Stale { etag: current }.into());
        }
        if row.status == ARCHIVED {
            return Err(ErrorKind::Archived.into());
        }
        let changed = Changed {
            status: patch
                .status
                .map_or(row.status.as_str(), |status| status.as_str()),
            name: patch.name.as_deref().unwrap_or(&row.name),
            model_ref: patch.model_ref.as_deref().unwrap_or(&row.model_ref),
            http_egress: patch.http_egress.map_or(row.egress()?, stored_egress),
            package_domains: patch.package_domains.unwrap_or(row.package_domains),
        };
        let model_changed = changed.model_ref != row.model_ref;
        if model_changed {
            model::eligible(&mut tx, &person.owner, changed.model_ref).await?;
        }
        update(&mut tx, access.workspace, fleet, &changed).await?;
        if model_changed {
            record_model_change(
                &mut tx,
                access.workspace,
                fleet,
                &row.model_ref,
                changed.model_ref,
            )
            .await?;
        }
        let cancelled = if changed.status == ARCHIVED {
            super::cancel_runs(&mut tx, fleet).await?
        } else {
            Vec::new()
        };
        tx.commit().await.map_err(query(CONTEXT))?;
        if model_changed {
            afd_turns::view::activity::announce(&self.turns.streams(), fleet).await;
        }
        self.announce_cancellations(cancelled).await;
        self.read(access, fleet).await
    }
}

/// The stored scope a requested toggle names.
const fn stored_egress(requested: af_wire::policy::HttpEgress) -> HttpEgress {
    match requested {
        af_wire::policy::HttpEgress::Restricted => HttpEgress::Restricted,
        af_wire::policy::HttpEgress::Public => HttpEgress::Public,
    }
}

/// Writes `changed` to the locked row; a name the workspace already holds is
/// refused as one.
async fn update(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    changed: &Changed<'_>,
) -> Result<()> {
    sqlx::query(sql::UPDATE)
        .bind(fleet)
        .bind(workspace)
        .bind(changed.status)
        .bind(changed.name)
        .bind(changed.model_ref)
        .bind(changed.http_egress.as_str())
        .bind(changed.package_domains)
        .execute(connection)
        .await
        .map_err(|source| {
            if afd_db::constraint::violates_unique(&source, NAME_KEY) {
                ErrorKind::NameExists.into()
            } else {
                query(CONTEXT)(source)
            }
        })?;
    Ok(())
}

/// The unique key a rename onto a taken name breaks.
const NAME_KEY: &str = "uq_fleets_workspace_id_name";

/// The history receipt a model change writes beside the row.
async fn record_model_change(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    previous: &str,
    current: &str,
) -> Result<()> {
    let payload = json!({ "previous_model_ref": previous, "model_ref": current });
    sqlx::query(afd_turns::view::sql::INSERT_TURN)
        .bind(fleet)
        .bind(None::<&Uuid7>)
        .bind(workspace)
        .bind(ACTOR_MODEL_CHANGE)
        .bind(EVENT_TYPE_MODEL_CHANGED)
        .bind(sqlx::types::Json(payload))
        .bind(None::<&Uuid7>)
        .bind(af_core::turn::status::PROCESSED)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
