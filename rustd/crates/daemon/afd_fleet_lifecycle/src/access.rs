//! Which fleets may read each workspace secret, from the policy a run enforces.
//!
//! A fleet reads a secret only once a person approved its grant:
//! `afd_credential::secrets::declared` resolves the approved names and nothing
//! else. This read projects the same grants, so "available to" is never
//! inferred from a request, a trigger or a name that merely looks related.

use std::collections::BTreeMap;

use af_core::id::Uuid7;
use afd_api_wire::grant::status::APPROVED;

use crate::error::{self, Result};
use crate::{FleetStatus, Fleets, sql};

/// What the access read was doing, for the operator's log line.
const CONTEXT_ACCESS: &str = "read secret access by fleet";

/// One fleet a secret is available to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFleet {
    /// The fleet's identifier.
    pub id: Uuid7,
    /// Its current name.
    pub name: String,
}

/// Secret name to the runnable fleets approved to read it, each list ordered by
/// name. A secret no runnable fleet holds has no entry.
pub type SecretAccess = BTreeMap<String, Vec<SecretFleet>>;

#[derive(sqlx::FromRow)]
struct Row {
    secret_name: String,
    fleet_id: Uuid7,
    fleet_name: String,
}

impl Fleets {
    /// Every secret name runnable fleets in `workspace` hold an approved grant
    /// for, with those fleets.
    ///
    /// One statement for the whole workspace, never one per secret or fleet.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn secret_access(&self, workspace: &Uuid7) -> Result<SecretAccess> {
        let mut connection = self.database.acquire().await?;
        let rows: Vec<Row> = sqlx::query_as(sql::SELECT_SECRET_ACCESS)
            .bind(workspace)
            .bind(FleetStatus::Active.as_str())
            .bind(APPROVED)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_ACCESS))?;
        Ok(rows
            .into_iter()
            .fold(SecretAccess::new(), |mut access, row| {
                access
                    .entry(row.secret_name)
                    .or_default()
                    .push(SecretFleet {
                        id: row.fleet_id,
                        name: row.fleet_name,
                    });
                access
            }))
    }
}
