//! The `fleet.fleets` row a suite seeds, spelled once.
//!
//! Most integration suites need a fleet to hang leases, events, grants or
//! schedules from. Each used to write its own `INSERT INTO` with
//! positional binds the compiler cannot check, so dropping one column meant
//! editing forty-odd suites by hand. They build a [`FleetSeed`] instead and the
//! column list changes here alone: the shape Arrow's `util::data_gen` gives its
//! fixtures, behind the same `test-util` flag (M-TEST-UTIL).
//!
//! `status` is an argument rather than a default because its spellings live
//! above this crate, in `afd_state::sql`. This crate owns the table's columns,
//! not the values the product compares against.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a seed may set. A column the caller leaves alone is bound to
/// the value a fresh fleet takes.
const INSERT_FLEET: &str = include_str!("sql/insert.sql");

/// [`INSERT_FLEET`] plus the setup revision the fleet points at, in one
/// statement so no reader sees the fleet without its setup. The revision is
/// stamped with the fleet's creation. An unstamped seed takes the database's
/// `now()`, the clock every lifecycle stamp comes from.
const INSERT_FLEET_WITH_SETUP: &str = include_str!("sql/insert_with_setup.sql");

/// The revision number a seeded setup takes when the seed names none.
const FIRST_SETUP_VERSION: i64 = 1;

/// The resource version a fleet is created at when the seed names none.
const FIRST_RESOURCE_VERSION: i64 = 1;

const CONTEXT: &str = "seed a fleet";

/// The native setup [`FleetSeed::native_setup`] selects, as stored.
///
/// It is what `afd_fleet_runtime`'s `FixtureSetup::default()` writes, which
/// that crate's suite pins against these four, since this crate cannot parse a
/// definition.
pub const FIXTURE_SETUP_DEFINITION: &str = "{\"schema_version\":1,\"description\":\"Runtime fixture.\",\"instructions_md\":\"Run the fixture.\",\"config\":{\"name\":\"fixture\",\"x-agentsfleet\":{\"triggers\":[],\"tools\":[],\"budget\":{\"daily_nanos\":\"1000000000\"}}}}";

/// [`FIXTURE_SETUP_DEFINITION`]'s canonical digest.
pub const FIXTURE_SETUP_DIGEST: &str =
    "definition:v1:sha256:1508c56080b66122da69a01a5102d5dcf47ff2a79d5e41a4cb697a193368dc61";

/// The policy semantics it is written under.
pub const FIXTURE_SETUP_SEMANTICS: &str = "fleet-setup-v1";

/// The runtime model it runs: the platform default, which `afd_credential`
/// names and `afd_fleet`'s suite pins.
pub const FIXTURE_SETUP_MODEL_REF: &str = "platform-default";

/// The placement tag a [`FleetSeed::native_setup`] fleet requires when its seed
/// names none, and that no fixture runner carries.
///
/// A fixture fleet with a readable setup admits work every runner could take,
/// and a suite that only admits leaves that work behind. In a shared lane the
/// next suite's runner would be handed it. Untagged, a fleet matches every
/// runner; this tag matches none, so admitted work stays with its own suite.
const UNPLACED: &[&str] = &["fixture-unplaced"];

/// The setup revision a seeded fleet selects, as the columns that store it.
///
/// The strings come from the crate that owns the definition format; this
/// crate stores them without reading them.
#[derive(Debug, Clone, Copy)]
pub struct SetupRevision<'a> {
    /// The definition, as stored JSON.
    pub definition: &'a str,
    /// Its canonical digest.
    pub digest: &'a str,
    /// The policy semantics it was written under.
    pub semantics: &'a str,
    /// The runtime model it runs, for a fleet the lease path reads.
    pub model_ref: Option<&'a str>,
}

/// One fleet row, built from its identity and its status.
#[derive(Debug, Clone)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct FleetSeed<'a> {
    id: &'a str,
    workspace_id: &'a str,
    tenant_id: &'a str,
    status: &'a str,
    name: &'a str,
    required_tags: &'a [&'a str],
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
    resource_version: Option<i64>,
    setup_version: Option<i64>,
    created_at: Option<Timestamp>,
    updated_at: Option<Timestamp>,
    setup: Option<SetupRevision<'a>>,
}

impl<'a> FleetSeed<'a> {
    /// A fleet named after its own id, which keeps the name unique within its
    /// workspace without the caller minting one. No setup, no tags, no
    /// memory grants, no setup counters, stamped by the database's `now()`.
    pub fn new(id: &'a str, workspace_id: &'a str, tenant_id: &'a str, status: &'a str) -> Self {
        Self {
            id,
            workspace_id,
            tenant_id,
            status,
            name: id,
            required_tags: &[],
            memory_reads_workspace: false,
            memory_publishes_workspace: false,
            resource_version: None,
            setup_version: None,
            created_at: None,
            updated_at: None,
            setup: None,
        }
    }

    /// Names the fleet, for a suite that asserts on the name or seeds two
    /// fleets it tells apart by it.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// The capability tags a runner must carry to be placed on the fleet.
    pub fn tags(mut self, tags: &'a [&'a str]) -> Self {
        self.required_tags = tags;
        self
    }

    /// The fleet's grants over workspace memory: reading it, and publishing to it.
    pub fn memory(mut self, reads: bool, publishes: bool) -> Self {
        self.memory_reads_workspace = reads;
        self.memory_publishes_workspace = publishes;
        self
    }

    /// The counters a saved fleet carries: its resource version, and the
    /// number of the setup revision [`FleetSeed::setup`] writes.
    pub fn versions(mut self, resource: i64, setup: i64) -> Self {
        self.resource_version = Some(resource);
        self.setup_version = Some(setup);
        self
    }

    /// The setup revision the fleet selects, which admission and the lease
    /// path both require. Numbers it [`FIRST_SETUP_VERSION`] unless
    /// [`FleetSeed::versions`] named one.
    pub fn setup(mut self, revision: SetupRevision<'a>) -> Self {
        self.setup = Some(revision);
        self
    }

    /// [`FleetSeed::setup`] with the fixture setup every suite shares: no
    /// triggers, no tools, a budget, on the platform model. A seed that named
    /// no tags takes [`UNPLACED`], so no runner from another suite is handed
    /// its work.
    pub fn native_setup(self) -> Self {
        self.unplaced_setup(SetupRevision {
            definition: FIXTURE_SETUP_DEFINITION,
            digest: FIXTURE_SETUP_DIGEST,
            semantics: FIXTURE_SETUP_SEMANTICS,
            model_ref: Some(FIXTURE_SETUP_MODEL_REF),
        })
    }

    /// [`FleetSeed::setup`] for a suite's own setup, taking [`UNPLACED`] when
    /// the seed named no tags, as [`FleetSeed::native_setup`] does.
    pub fn unplaced_setup(mut self, revision: SetupRevision<'a>) -> Self {
        if self.required_tags.is_empty() {
            self.required_tags = UNPLACED;
        }
        self.setup(revision)
    }

    /// Stamps `created_at` and `updated_at` with the same instant, for a
    /// suite that orders rows by when they were made.
    pub fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self.updated_at = Some(instant);
        self
    }

    /// Restamps `updated_at` alone, for a fleet edited after it was created.
    pub fn updated(mut self, instant: Timestamp) -> Self {
        self.updated_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a missing workspace or a
    /// duplicate name among them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        let revision_id = super::mint_id();
        let statement = if self.setup.is_some() {
            INSERT_FLEET_WITH_SETUP
        } else {
            INSERT_FLEET
        };
        let mut query = sqlx::query(statement)
            .bind(self.id)
            .bind(self.workspace_id)
            .bind(self.tenant_id)
            .bind(self.name)
            .bind(self.status)
            .bind(self.required_tags)
            .bind(self.memory_reads_workspace)
            .bind(self.memory_publishes_workspace)
            .bind(self.resource_version.unwrap_or(FIRST_RESOURCE_VERSION))
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .bind(self.updated_at.map(Timestamp::to_sqlx));
        if let Some(setup) = self.setup {
            query = query
                .bind(revision_id)
                .bind(self.setup_version.unwrap_or(FIRST_SETUP_VERSION))
                .bind(setup.definition)
                .bind(setup.digest)
                .bind(setup.semantics)
                .bind(setup.model_ref);
        }
        query
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
