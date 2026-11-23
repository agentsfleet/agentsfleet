//! The onboarding checklist: five signals derived from what a workspace holds.
//!
//! Nothing here is stored for the checklist. Every flag is an `EXISTS` over a
//! table that holds the thing it names, so the checklist cannot disagree with
//! the workspace it describes.

use af_core::id::Uuid7;
use afd_db::Db;

use crate::sql::onboarding as sql;
use crate::{Result, error};

/// The actor prefix a steer event carries.
///
/// `steer:` is what the messages handler stamps; the `%` makes it a prefix
/// match the index can serve. Bound as a parameter, never inlined (RULE NSQ).
const STEER_ACTOR_LIKE: &str = "steer:%";

const CONTEXT_SIGNALS: &str = "onboarding.signals";
const CONTEXT_PLATFORM_MODEL: &str = "onboarding.platform_default_model";

/// The five signals a workspace's onboarding checklist is derived from.
///
/// Five independent booleans, and `struct_excessive_bools` is right that this
/// usually means a missing type. Not here: each flag answers a different
/// table, and no two of them combine into a state worth naming — a workspace
/// can hold a fleet with no secret, or a steer event with no model. Collapsing
/// them into an enum would invent states the schema cannot produce.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per table the checklist reads; see above"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signals {
    /// The workspace holds at least one fleet.
    pub has_fleet: bool,
    /// The workspace holds at least one vault secret.
    pub has_secret: bool,
    /// At least one event has reached the workspace.
    pub has_processed_event: bool,
    /// At least one of those events came from a steer.
    pub has_steer_event: bool,
    /// A model is resolvable — the tenant's own, or the platform default.
    pub model_configured: bool,
}

/// Reads a workspace's onboarding checklist.
///
/// Holds the api-role pool: the read is on a dashboard request path, and a
/// request-path read sharing a pool with background work waits behind it.
#[derive(Debug, Clone)]
pub struct Onboarding {
    database: Db,
}

impl Onboarding {
    /// A checklist reader over `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// Every derivable onboarding signal for one workspace.
    ///
    /// `tenant` scopes the model check; the other four are workspace-scoped.
    /// `model_configured` is true when the tenant holds its own non-empty
    /// selection OR an active platform default does — false only when no model
    /// exists anywhere, which is the only state the checklist should nag about.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn signals(&self, workspace: &Uuid7, tenant: &Uuid7) -> Result<Signals> {
        let mut connection = self.database.acquire().await?;
        let row: SignalsRow = sqlx::query_as(sql::SELECT_SIGNALS)
            .bind(workspace)
            .bind(STEER_ACTOR_LIKE)
            .bind(tenant)
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_SIGNALS))?;

        let model_configured = if row.tenant_model {
            true
        } else {
            sqlx::query_scalar(sql::SELECT_PLATFORM_DEFAULT_MODEL)
                .fetch_one(&mut *connection)
                .await
                .map_err(error::query(CONTEXT_PLATFORM_MODEL))?
        };

        Ok(Signals {
            has_fleet: row.has_fleet,
            has_secret: row.has_secret,
            has_processed_event: row.has_event,
            has_steer_event: row.has_steer,
            model_configured,
        })
    }
}

/// The signals statement's row, by the names it aliases each `EXISTS`.
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per `EXISTS` the statement answers; `Signals` explains why"
)]
#[derive(Debug, sqlx::FromRow)]
struct SignalsRow {
    has_fleet: bool,
    has_secret: bool,
    has_event: bool,
    has_steer: bool,
    tenant_model: bool,
}
