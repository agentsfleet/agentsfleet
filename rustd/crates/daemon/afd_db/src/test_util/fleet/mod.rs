//! The `fleet.fleets` row a suite seeds, spelled once.
//!
//! Most integration suites need a fleet to hang runs, events, grants or
//! schedules from. Each used to write its own `INSERT INTO` with
//! positional binds the compiler cannot check, so dropping one column meant
//! editing forty-odd suites by hand. They build a [`FleetSeed`] instead and the
//! column list changes here alone: the shape Arrow's `util::data_gen` gives its
//! fixtures, behind the same `test-util` flag (M-TEST-UTIL).
//!
//! `status` is an argument rather than a default because its spellings live
//! above this crate, in `afd_state::sql`. This crate owns the table's columns,
//! not the values the product compares against; the two defaults it does bind
//! — a model and a restricted network — are pinned against their owners in
//! this crate's suite.

use af_core::id::Uuid7;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a seed may set, and the fleet's thread written with it. A
/// column the caller leaves alone is bound to the value a fresh fleet takes.
const INSERT_FLEET: &str = include_str!("sql/insert.sql");

/// The resource version a fleet is created at when the seed names none.
const FIRST_RESOURCE_VERSION: i64 = 1;

const CONTEXT: &str = "seed a fleet";

/// The model a seeded fleet runs when the seed names none: the platform
/// default, which `afd_credential` names and `afd_fleet`'s suite pins.
pub const FIXTURE_MODEL_REF: &str = "platform-default";

/// The network a fresh fleet takes: HTTP tools restricted to the hosts of its
/// approved access, as `af_fleet_runtime::config::HttpEgress::Restricted`
/// spells it.
pub const FIXTURE_HTTP_EGRESS: &str = "restricted";

/// The placement tag a seeded fleet requires until its seed names its own,
/// and that no fixture runner carries.
///
/// Every fleet admits work every runner could take, and a suite that only
/// admits leaves that work behind. In a shared lane the next suite's runner
/// would be handed it. Untagged, a fleet matches every runner; this tag
/// matches none, so admitted work stays with its own suite. A suite whose
/// runners take the work names the tag they carry with [`FleetSeed::tags`].
const UNPLACED: &[&str] = &["fixture-unplaced"];

/// One fleet row, built from its identity and its status.
#[derive(Debug, Clone)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct FleetSeed<'a> {
    id: &'a Uuid7,
    workspace_id: &'a Uuid7,
    tenant_id: &'a Uuid7,
    status: &'a str,
    name: &'a str,
    model_ref: &'a str,
    http_egress: &'a str,
    package_domains: bool,
    required_tags: &'a [&'a str],
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
    resource_version: Option<i64>,
    created_at: Option<Timestamp>,
    updated_at: Option<Timestamp>,
}

impl<'a> FleetSeed<'a> {
    /// A fleet named after its own id, which keeps the name unique within its
    /// workspace without the caller minting one. The fixture model, a
    /// restricted network with no package registries, the [`UNPLACED`] tag,
    /// no memory grants, stamped by the database's `now()`.
    pub fn new(
        id: &'a Uuid7,
        workspace_id: &'a Uuid7,
        tenant_id: &'a Uuid7,
        status: &'a str,
    ) -> Self {
        Self {
            id,
            workspace_id,
            tenant_id,
            status,
            name: id.as_str(),
            model_ref: FIXTURE_MODEL_REF,
            http_egress: FIXTURE_HTTP_EGRESS,
            package_domains: false,
            required_tags: UNPLACED,
            memory_reads_workspace: false,
            memory_publishes_workspace: false,
            resource_version: None,
            created_at: None,
            updated_at: None,
        }
    }

    /// Names the fleet, for a suite that asserts on the name or seeds two
    /// fleets it tells apart by it.
    pub const fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// The workspace model the fleet's runs resolve.
    pub const fn model(mut self, model_ref: &'a str) -> Self {
        self.model_ref = model_ref;
        self
    }

    /// The fleet's network toggles: its HTTP egress spelling, and whether the
    /// package registries are open to it.
    pub const fn network(mut self, http_egress: &'a str, package_domains: bool) -> Self {
        self.http_egress = http_egress;
        self.package_domains = package_domains;
        self
    }

    /// The capability tags a runner must carry to be placed on the fleet, in
    /// place of [`UNPLACED`]. An empty list matches every runner in the lane.
    pub const fn tags(mut self, tags: &'a [&'a str]) -> Self {
        self.required_tags = tags;
        self
    }

    /// Takes [`UNPLACED`] back when the tags named are none, so no runner
    /// from another suite is handed the fleet's work; named tags stay.
    pub const fn unplaced(mut self) -> Self {
        if self.required_tags.is_empty() {
            self.required_tags = UNPLACED;
        }
        self
    }

    /// The fleet's grants over workspace memory: reading it, and publishing to it.
    pub const fn memory(mut self, reads: bool, publishes: bool) -> Self {
        self.memory_reads_workspace = reads;
        self.memory_publishes_workspace = publishes;
        self
    }

    /// The resource version a changed fleet carries.
    pub const fn version(mut self, resource: i64) -> Self {
        self.resource_version = Some(resource);
        self
    }

    /// Stamps `created_at` and `updated_at` with the same instant, for a
    /// suite that orders rows by when they were made.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self.updated_at = Some(instant);
        self
    }

    /// Restamps `updated_at` alone, for a fleet edited after it was created.
    pub const fn updated(mut self, instant: Timestamp) -> Self {
        self.updated_at = Some(instant);
        self
    }

    /// Writes the row and its thread.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a missing workspace or a
    /// duplicate name among them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_FLEET)
            .bind(self.id)
            .bind(self.workspace_id)
            .bind(self.tenant_id)
            .bind(self.name)
            .bind(self.status)
            .bind(self.model_ref)
            .bind(self.http_egress)
            .bind(self.package_domains)
            .bind(self.required_tags)
            .bind(self.memory_reads_workspace)
            .bind(self.memory_publishes_workspace)
            .bind(self.resource_version.unwrap_or(FIRST_RESOURCE_VERSION))
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .bind(self.updated_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
