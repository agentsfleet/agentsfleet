//! The `core.fleets` row a suite seeds, spelled once.
//!
//! Most integration suites need a fleet to hang leases, events, grants or
//! schedules from. Each used to write its own `INSERT INTO core.fleets` with
//! positional binds the compiler cannot check, so dropping one column meant
//! editing forty-odd suites by hand. They build a [`FleetSeed`] instead and the
//! column list changes here alone: the shape Arrow's `util::data_gen` gives its
//! fixtures, behind the same `test-util` flag (M-TEST-UTIL).
//!
//! `status` is an argument rather than a default because its spellings live
//! above this crate, in `afd_state::sql`. This crate owns the table's columns,
//! not the values the product compares against.

use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a seed may set. A column the caller leaves alone is bound to
/// the value the table would default it to.
const INSERT_FLEET: &str = "INSERT INTO core.fleets \
     (id, workspace_id, tenant_id, name, config_json, status, required_tags, \
      memory_reads_workspace, memory_publishes_workspace, resource_version, setup_version, \
      created_at, updated_at) \
     VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5::jsonb, $6, $7::text[], \
      $8, $9, $10, $11, $12, $13)";

/// [`INSERT_FLEET`] plus the setup revision the fleet selects, in one
/// statement so no reader sees the fleet without its setup. The revision's
/// number is the fleet's `setup_version` and its stamp the fleet's creation.
const INSERT_FLEET_WITH_SETUP: &str = "WITH fleet AS (INSERT INTO core.fleets \
     (id, workspace_id, tenant_id, name, config_json, status, required_tags, \
      memory_reads_workspace, memory_publishes_workspace, resource_version, setup_version, \
      created_at, updated_at, current_definition_revision_id) \
     VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5::jsonb, $6, $7::text[], \
      $8, $9, $10, $11, $12, $13, $14::uuid) RETURNING id) \
     INSERT INTO core.fleet_definition_revisions \
     (id, fleet_id, revision, definition, canonical_digest, policy_semantics, created_at, \
      runtime_model_ref) \
     SELECT $14::uuid, id, $11, $15::jsonb, $16, $17, $12, $18 FROM fleet";

/// The revision number a seeded setup takes when the seed names none.
const FIRST_SETUP_VERSION: i64 = 1;

const CONTEXT: &str = "seed a fleet";

/// The native setup [`FleetSeed::native_setup`] selects, as stored: what
/// `afd_fleet_runtime`'s `FixtureSetup::default()` writes, which that crate's
/// suite pins against these four, since this crate cannot parse a definition.
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

/// The config of a fleet whose config the test never reads. Structural, like
/// the table's own `'{}'` tag default, not a vocabulary value.
const EMPTY_CONFIG: &str = "{}";

/// One fleet row, built from its identity and its status.
#[derive(Debug, Clone)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct FleetSeed<'a> {
    id: &'a str,
    workspace_id: &'a str,
    tenant_id: &'a str,
    status: &'a str,
    name: &'a str,
    config_json: &'a str,
    required_tags: &'a [&'a str],
    memory_reads_workspace: bool,
    memory_publishes_workspace: bool,
    resource_version: Option<i64>,
    setup_version: Option<i64>,
    created_at: i64,
    updated_at: i64,
    setup: Option<SetupRevision<'a>>,
}

impl<'a> FleetSeed<'a> {
    /// A fleet named after its own id, which keeps the name unique within its
    /// workspace without the caller minting one. Empty config, no tags, no
    /// memory grants, no setup counters, stamped at zero.
    pub fn new(id: &'a str, workspace_id: &'a str, tenant_id: &'a str, status: &'a str) -> Self {
        Self {
            id,
            workspace_id,
            tenant_id,
            status,
            name: id,
            config_json: EMPTY_CONFIG,
            required_tags: &[],
            memory_reads_workspace: false,
            memory_publishes_workspace: false,
            resource_version: None,
            setup_version: None,
            created_at: 0,
            updated_at: 0,
            setup: None,
        }
    }

    /// Names the fleet, for a suite that asserts on the name or seeds two
    /// fleets it tells apart by it.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = name;
        self
    }

    /// The config the fleet carries, as JSON text.
    pub fn config(mut self, json: &'a str) -> Self {
        self.config_json = json;
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

    /// The counters a saved fleet carries.
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
    pub fn native_setup(mut self) -> Self {
        if self.required_tags.is_empty() {
            self.required_tags = UNPLACED;
        }
        self.setup(SetupRevision {
            definition: FIXTURE_SETUP_DEFINITION,
            digest: FIXTURE_SETUP_DIGEST,
            semantics: FIXTURE_SETUP_SEMANTICS,
            model_ref: Some(FIXTURE_SETUP_MODEL_REF),
        })
    }

    /// Stamps `created_at` and `updated_at` with the same instant.
    pub fn at(mut self, millis: i64) -> Self {
        self.created_at = millis;
        self.updated_at = millis;
        self
    }

    /// Restamps `updated_at` alone, for a fleet edited after it was created.
    pub fn updated(mut self, millis: i64) -> Self {
        self.updated_at = millis;
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails, a missing workspace or a
    /// duplicate name among them.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        let revision_id = super::mint_id();
        let setup_version = self.setup.map_or(self.setup_version, |_| {
            Some(self.setup_version.unwrap_or(FIRST_SETUP_VERSION))
        });
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
            .bind(self.config_json)
            .bind(self.status)
            .bind(self.required_tags)
            .bind(self.memory_reads_workspace)
            .bind(self.memory_publishes_workspace)
            .bind(self.resource_version)
            .bind(setup_version)
            .bind(self.created_at)
            .bind(self.updated_at);
        if let Some(setup) = self.setup {
            query = query
                .bind(revision_id)
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
