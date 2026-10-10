//! Onboarding into a workspace's own library, over the same pipeline.
//!
//! # A second catalogue, not a second pipeline
//!
//! [`ImportService`] already takes its destination as a type parameter, so the
//! fetch, the validation, the content hash and the object-store snapshot are
//! reached identically whichever library a bundle lands in. What differs is one
//! statement and one row shape, and that is exactly what
//! [`BundleCatalog`](crate::BundleCatalog) is the seam for — the platform
//! catalogue beside this one is the other implementation.
//!
//! # What the tenant tier does differently, and why
//!
//! It mints a UUID rather than claiming a slug. The platform catalogue is
//! curated and its `id` IS the name an operator publishes; a workspace onboards
//! at runtime and two workspaces may onboard the same repository, so a shared
//! name would be a collision between tenants that neither can see or resolve.
//!
//! It therefore has no collision refusal and no `replace` flag. Onboarding the
//! same bundle twice into one workspace is ONE entry — the domain key is
//! `(workspace_id, content_hash)` and the upsert refreshes the row it finds —
//! so there is nothing for a caller to force past. That asymmetry is what
//! [`Destination`](super::Destination) makes unrepresentable rather than
//! documented: only the platform arm carries a `replace`.
//!
//! It lands visible. A platform row arrives as a draft because an operator
//! publishes it afterwards; a workspace's own entry has no such review step,
//! and a row nobody could see would be an onboarding that silently did nothing.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;

use crate::error::{Error, ErrorKind, database};
use crate::{BundleCatalog, ImportBody, PreparedBundle, Result};

/// The context a failed tenant onboarding reports under.
const CONTEXT_ONBOARD: &str = "onboard a workspace Fleet Bundle";

/// The tier a workspace's own entry is stored and rendered under.
///
/// The same spelling [`Tier::Tenant`](crate::Tier) renders, and it is stored in
/// the row rather than derived on read so the gallery's union arm can project a
/// column that already agrees with it.
const VISIBILITY_TENANT: &str = "tenant";

/// Where an onboarded bundle lands in a workspace's own library.
#[derive(Debug)]
pub(super) struct TenantCatalog {
    pub(super) database: Db,
    pub(super) workspace: Uuid7,
    pub(super) entropy: Entropy,
    pub(super) now: UnixMillis,
}

impl BundleCatalog for TenantCatalog {
    async fn insert(&self, body: &ImportBody, bundle: &PreparedBundle) -> Result<String> {
        let requirements = serde_json::to_string(&bundle.requirements)?;
        let support_files = serde_json::to_string(&bundle.support_manifest)?;
        let skill = super::markdown("SKILL.md", &body.skill_markdown)?;
        let trigger = body
            .trigger_markdown
            .as_deref()
            .map(|value| super::markdown("TRIGGER.md", value))
            .transpose()?;

        let mut connection = self.database.acquire().await?;
        let id: String = sqlx::query_scalar(INSERT_OR_EXISTING)
            .bind(self.mint()?.as_str())
            .bind(self.workspace.as_str())
            .bind(&bundle.name)
            .bind(&bundle.description)
            .bind(body.source_kind.as_str())
            .bind(&body.source_ref)
            .bind(VISIBILITY_TENANT)
            .bind(&bundle.content_hash)
            .bind(skill)
            .bind(trigger)
            .bind(support_files)
            .bind(requirements)
            .bind(self.now.as_millis())
            .fetch_one(&mut *connection)
            .await
            .map_err(database(CONTEXT_ONBOARD))?;
        Ok(id)
    }
}

impl TenantCatalog {
    /// A fresh identifier for the row this onboarding may create.
    ///
    /// Minted here rather than by the database: the table's own check constrains
    /// `id` to a version-7 UUID, and Postgres has no generator for one in this
    /// schema. Drawn on every call even when the upsert then finds an existing
    /// row — a wasted identifier costs nothing, where reading first to decide
    /// whether to mint would be the check-then-act the upsert exists to avoid.
    fn mint(&self) -> Result<Uuid7> {
        self.entropy
            .uuid7(self.now)
            .map_err(|source| Error::from(ErrorKind::Entropy { source }))
    }
}

/// Stores a workspace's onboarded bundle, or answers the one it already had.
///
/// `$1` id · `$2` workspace · `$3` name · `$4` description · `$5` source kind ·
/// `$6` source ref · `$7` tier · `$8` content hash · `$9` skill · `$10` trigger ·
/// `$11` support manifest · `$12` requirements · `$13` now.
///
/// `ON CONFLICT (workspace_id, content_hash) DO NOTHING` is the domain key
/// asserting that the same bytes onboarded twice into one workspace are ONE
/// entry — and the second call CHANGES NOTHING rather than refreshing the row.
/// That is deliberate: the content hash is what the key is made of, so a row it
/// matches already holds those exact bytes and everything derived from them.
/// The only columns a refresh could actually move are `source_kind` and
/// `source_ref`, and rewriting those would rename the provenance of an entry a
/// client already holds an id for.
///
/// The `UNION ALL` is what makes this one round trip instead of two: `DO
/// NOTHING` returns no row on the conflict path, so the second arm reads back
/// the id that already stands. `LIMIT 1` because the domain key admits one.
///
/// `id` is a freshly minted uuidv7 on every call, so the primary key can never
/// be the arbiter that conflicts. One unique index is ever in play, which is
/// what keeps this clear of the unprincipled-deadlock class that `ON CONFLICT`
/// across several indexes lives in — and a minted identifier the conflict path
/// discards costs nothing.
const INSERT_OR_EXISTING: &str = "\
WITH inserted AS (
  INSERT INTO core.tenant_fleet_library
    (id, workspace_id, name, description, source_kind, source_ref, visibility,
     content_hash, skill_markdown, trigger_markdown, support_files_json,
     requirements_json, created_at, updated_at)
  VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12::jsonb, $13, $13)
  ON CONFLICT (workspace_id, content_hash) DO NOTHING
  RETURNING id::text
)
SELECT id FROM inserted
UNION ALL
SELECT id::text FROM core.tenant_fleet_library
 WHERE workspace_id = $2::uuid AND content_hash = $8
 LIMIT 1";
