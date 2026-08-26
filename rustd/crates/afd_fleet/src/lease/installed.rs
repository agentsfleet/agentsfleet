//! The claim read: what a fleet IS, at the moment a runner takes its slot.
//!
//! One statement, run once per claim. [`super::assign`] has already decided
//! WHICH fleet; this reads the installed shape that fleet's lease is built
//! from — the config the gates judge against, the prose the run is given, the
//! bundle it materialises, and the session it continues.
//!
//! # Why the config is re-read per lease and never cached
//!
//! A fleet's config is what the money gates price, what the approval gate
//! judges, and what the execution policy is assembled from. An operator who
//! narrows a fleet's tools or lowers its budget expects the NEXT run to obey
//! the change; a cache would make that "some later run, once an entry expired",
//! and the window would be invisible. The read costs one round trip on a path
//! that already makes several.

use afd_core::id::Uuid7;
use afd_fleet_runtime::FleetConfig;
use sqlx::Row as _;

use crate::error::{Result, query, row_malformed};
use crate::lease::assign::FLEET_STATUS_ACTIVE;
use crate::lease::store::Leases;
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_INSTALLED: &str = "fleet claim read";

/// The table a malformed value is reported against.
const TABLE_FLEETS: &str = "core.fleets";

/// The session context a fleet with no checkpoint starts from.
///
/// `fleet_session.zig`'s `S_FRESH_CONTEXT`. An empty JSON OBJECT rather than
/// `null` or an empty string: the runner reads this as a context document, and
/// a fleet on its first run has an empty one rather than a missing one.
pub const FRESH_CONTEXT: &str = "{}";

/// A fleet as installed, resolved for one lease.
#[derive(Debug, Clone)]
pub struct Installed {
    /// The workspace that owns it.
    pub workspace_id: Uuid7,
    /// Its name, as the operator typed it.
    pub name: String,
    /// The typed config the gates and the policy are built from.
    pub config: FleetConfig,
    /// The behaviour prose the run is given.
    pub instructions: String,
    /// The session context this run continues, or [`FRESH_CONTEXT`].
    pub context_json: String,
    /// The bundle to materialise, when the fleet was created from one.
    pub bundle_content_hash: Option<String>,
}

impl Leases {
    /// The installed fleet behind `fleet_id`, if it is still runnable.
    ///
    /// `None` means the fleet is no longer `active`. That is not an error and
    /// not a missing row: the selection pass filters on status, so reaching
    /// here with a stopped fleet means an operator paused it in the window
    /// between selection and this read. The caller answers no-work and the
    /// claim lapses on its own — which is the same thing that happens to every
    /// other fleet an operator stops.
    ///
    /// A fleet whose row is gone answers `None` for the same reason: there is
    /// nothing to run and nothing for an operator to fix.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a `workspace_id` that will
    /// not parse as a UUID, and a `config_json` this daemon cannot read. The
    /// last is deliberately fatal to the lease rather than skipped: a fleet
    /// whose stored config is unreadable must not run under a config this
    /// daemon guessed at.
    pub async fn installed(&self, fleet_id: &Uuid7) -> Result<Option<Installed>> {
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::fleet::SELECT_FLEET_WITH_SESSION)
            .bind(fleet_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_INSTALLED))?;
        let Some(row) = row else {
            return Ok(None);
        };

        let status: String = row.try_get(3).map_err(query(CONTEXT_INSTALLED))?;
        if status != FLEET_STATUS_ACTIVE {
            return Ok(None);
        }

        let workspace: String = row.try_get(0).map_err(query(CONTEXT_INSTALLED))?;
        let document: String = row.try_get(1).map_err(query(CONTEXT_INSTALLED))?;
        let source_markdown: String = row.try_get(2).map_err(query(CONTEXT_INSTALLED))?;
        let bundle_content_hash: Option<String> =
            row.try_get(4).map_err(query(CONTEXT_INSTALLED))?;
        let name: String = row.try_get(5).map_err(query(CONTEXT_INSTALLED))?;
        let context_json: Option<String> = row.try_get(6).map_err(query(CONTEXT_INSTALLED))?;

        Ok(Some(Installed {
            workspace_id: Uuid7::parse(&workspace)
                .map_err(row_malformed(TABLE_FLEETS, "workspace_id"))?,
            name,
            config: FleetConfig::stored(&document)?,
            instructions: afd_fleet_runtime::instructions(&source_markdown).to_owned(),
            context_json: context_json.unwrap_or_else(|| FRESH_CONTEXT.to_owned()),
            bundle_content_hash,
        }))
    }
}
