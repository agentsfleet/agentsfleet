//! A fleet's create, read, change and delete.
//!
//! A person's first message writes the fleet, its thread and its first turn in
//! one transaction ([`create`]); nothing is set up before it works. A change
//! takes the row's strong tag, and a delete ends its live runs and removes it.

mod create;
mod delete;
mod model;
mod patch;
mod read;
mod sql;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_approval::IntegrationGrants;
use afd_db::Db;
use afd_turns::Turns;
use sqlx::PgConnection;

use crate::{
    Result,
    error::{ErrorKind, raise::query},
    tuning::SetupTuning,
};

/// A workspace's fleets as a person creates, reads, changes and deletes them,
/// and the model choices a fleet may run on.
#[derive(Debug, Clone)]
pub struct Creations {
    pub(crate) database: Db,
    pub(crate) entropy: Entropy,
    turns: Turns,
    grants: IntegrationGrants,
    pub(crate) tuning: SetupTuning,
    /// The deployment's list page (`[api]`): what a model list serves.
    pub(crate) list_page: af_validate::Ceiling,
}

impl Creations {
    /// Shares the daemon's pool, entropy and turn ledger. The ledger's bus
    /// carries the wake a new fleet's first turn rings and the notices a
    /// workspace's open walls read.
    #[must_use]
    pub fn new(database: Db, entropy: Entropy, turns: Turns) -> Self {
        Self {
            grants: IntegrationGrants::new(database.clone(), entropy.clone()),
            database,
            entropy,
            turns,
            tuning: SetupTuning::default(),
            list_page: af_core::paging::CEILING,
        }
    }

    /// Serves model lists under the deployment's list page.
    #[must_use]
    pub const fn with_list_page(mut self, ceiling: af_validate::Ceiling) -> Self {
        self.list_page = ceiling;
        self
    }

    /// Runs under the deployment's `[setup]` settings.
    #[must_use]
    pub const fn with_tuning(mut self, tuning: SetupTuning) -> Self {
        self.tuning = tuning;
        self
    }

    fn mint(&self, now: jiff::Timestamp) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(now)?)
    }

    /// Sends Cancel to each runner whose run a commit just marked. A hint: the
    /// durable mark or the missing row refuses the runner's next renewal
    /// either way.
    async fn announce_cancellations(&self, cancelled: Vec<(Uuid7, Uuid7)>) {
        let marked = cancelled
            .into_iter()
            .map(|(runner, run)| afd_turns::view::stop::Marked { runner, run });
        afd_turns::view::stop::announce(&self.turns.streams(), marked).await;
    }
}

const CONTEXT: &str = "create, read or change a fleet";
// Spelled by the lifecycle's own enum, so the fleet row can never name a
// status two ways.
const ACTIVE: &str = afd_fleet_lifecycle::FleetStatus::Active.as_str();
const ARCHIVED: &str = afd_fleet_lifecycle::FleetStatus::Archived.as_str();
const INITIAL_VERSION: i64 = 1;

fn etag(id: &Uuid7, version: i64) -> String {
    af_core::etag::versioned(id, version)
}

/// Refuses `fleet` unless `workspace` holds it; locks nothing.
pub(crate) async fn exists(
    connection: &mut PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<()> {
    let found: Option<Uuid7> = sqlx::query_scalar(sql::EXISTS)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    found
        .map(drop)
        .ok_or_else(|| ErrorKind::FleetMissing.into())
}

/// Marks `fleet`'s live runs cancelled on the caller's transaction, answering
/// each `(runner, run)` to tell after the commit.
async fn cancel_runs(connection: &mut PgConnection, fleet: &Uuid7) -> Result<Vec<(Uuid7, Uuid7)>> {
    sqlx::query_as(sql::CANCEL_RUNS)
        .bind(fleet)
        .bind(afd_state::sql::RUN_STATUS_ACTIVE)
        .fetch_all(connection)
        .await
        .map_err(query(CONTEXT))
}
