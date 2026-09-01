//! Platform onboarding over validated bundle inputs.

mod tenant;

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use object_store::ObjectStore;
use sqlx::Row as _;

use super::VISIBILITY_DRAFT;
use crate::{
    BundleCatalog, BundleSource, Error, GithubSource, ImportBody, ImportService, Onboarded,
    PreparedBundle, Result, SourceImporter, SourceKind,
};

const CONTEXT_IMPORT: &str = "import platform Fleet Bundle";
const CONTEXT_COLLISION: &str = "read Fleet Bundle collision owner";
const DEFAULT_REVISION: &str = "main";
const TEMPLATE_OWNER: &str = "agentsfleet";

/// Which library an onboarded bundle lands in.
///
/// A value rather than a pair of flags, because the two tiers do not take the
/// same arguments and never could. `replace` is the platform arm's alone: that
/// catalogue is keyed by the bundle's own name, so a second source claiming an
/// existing name is a collision an operator may choose to force past. A
/// workspace's library is keyed by `(workspace_id, content_hash)`, where the
/// same bytes onboarded twice are ONE entry the upsert refreshes — there is
/// nothing to force. Making that an enum rather than an ignored parameter is
/// `M-STRONG-TYPES` applied to an asymmetry a comment would otherwise carry.
#[derive(Debug, Clone, Copy)]
pub enum Destination<'a> {
    /// The operator-curated catalogue, staged as a draft.
    Platform {
        /// Whether to overwrite a name a different source already owns.
        replace: bool,
    },
    /// One workspace's own library, visible as soon as it lands.
    Workspace(&'a Uuid7),
}

/// Platform importer sharing the daemon's one snapshot-store handle.
#[derive(Debug, Clone)]
pub struct LibraryImports {
    database: Db,
    store: Option<Arc<dyn ObjectStore>>,
    /// Draws the identifier a workspace entry is minted with.
    ///
    /// Injected rather than constructed where it is used, because `Entropy` is
    /// this workspace's `M-MOCKABLE-SYSCALLS` source: a test that could not
    /// pin the identifier could not assert the row an onboarding wrote.
    entropy: Entropy,
    #[cfg(feature = "test-util")]
    github_api_base: Option<Box<str>>,
}

impl LibraryImports {
    /// Uses a configured snapshot store for bundles carrying support files.
    #[must_use]
    pub fn new(database: Db, store: Arc<dyn ObjectStore>, entropy: Entropy) -> Self {
        Self {
            database,
            store: Some(store),
            entropy,
            #[cfg(feature = "test-util")]
            github_api_base: None,
        }
    }

    /// Keeps skill-only onboarding available when snapshot storage is absent.
    #[must_use]
    pub const fn without_store(database: Db, entropy: Entropy) -> Self {
        Self {
            database,
            store: None,
            entropy,
            #[cfg(feature = "test-util")]
            github_api_base: None,
        }
    }

    /// Redirects GitHub fetches to a test-owned HTTP origin.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn with_github_api_base(mut self, api_base: impl Into<Box<str>>) -> Self {
        self.github_api_base = Some(api_base.into());
        self
    }

    /// Validates and persists an inline upload.
    ///
    /// # Errors
    /// Reports validation, storage, collision, or database failures.
    pub async fn upload(
        &self,
        body: &ImportBody,
        into: Destination<'_>,
        now: UnixMillis,
    ) -> Result<Onboarded> {
        self.persist(body, into, now).await
    }

    /// Fetches a public GitHub repository, then validates and persists it.
    ///
    /// # Errors
    /// Reports source, validation, storage, collision, or database failures.
    pub async fn github(
        &self,
        repository: &str,
        revision: Option<&str>,
        into: Destination<'_>,
        now: UnixMillis,
    ) -> Result<Onboarded> {
        let source = self.github_source(revision.unwrap_or(DEFAULT_REVISION))?;
        // Awaited inside each arm rather than after the match: the two are
        // different concrete futures, and only the value they resolve to is
        // shared. Boxing them to share one type would allocate per import to
        // save two `.await`s.
        match self.service(into, now) {
            Service::Platform(service) => {
                SourceImporter::new(source, service)
                    .import(repository)
                    .await
            }
            Service::Workspace(service) => {
                SourceImporter::new(source, service)
                    .import(repository)
                    .await
            }
        }
    }

    /// Fetches one first-party template from its fixed GitHub repository.
    ///
    /// # Errors
    /// Reports source, validation, storage, collision, or database failures.
    pub async fn template(
        &self,
        template: &str,
        into: Destination<'_>,
        now: UnixMillis,
    ) -> Result<Onboarded> {
        let repository = format!("{TEMPLATE_OWNER}/{template}");
        let source = self.github_source(DEFAULT_REVISION)?;
        let mut body = source.fetch(&repository).await?;
        body.source_kind = SourceKind::Template;
        body.source_ref = template.to_owned();
        body.source_revision = None;
        self.persist(&body, into, now).await
    }

    async fn persist(
        &self,
        body: &ImportBody,
        into: Destination<'_>,
        now: UnixMillis,
    ) -> Result<Onboarded> {
        match self.service(into, now) {
            Service::Platform(service) => service.import(body).await,
            Service::Workspace(service) => service.import(body).await,
        }
    }

    fn github_source(&self, revision: &str) -> Result<GithubSource> {
        let source = GithubSource::new(revision)?;
        #[cfg(feature = "test-util")]
        if let Some(api_base) = &self.github_api_base {
            return Ok(source.pointed_at(api_base.to_string()));
        }
        Ok(source)
    }

    /// The pipeline, bound to the catalogue its destination names.
    ///
    /// Two concrete services rather than a `Box<dyn BundleCatalog>`: the
    /// implementations are this crate's own and closed, which is the enum arm
    /// of `M-DI-HIERARCHY` rather than the `dyn` one — no allocation, and each
    /// catalogue keeps its own fields instead of both carrying the other's.
    fn service(&self, into: Destination<'_>, now: UnixMillis) -> Service {
        match into {
            Destination::Platform { replace } => Service::Platform(self.staged(PlatformCatalog {
                database: self.database.clone(),
                replace,
                now,
            })),
            Destination::Workspace(workspace) => {
                Service::Workspace(self.staged(tenant::TenantCatalog {
                    database: self.database.clone(),
                    workspace: workspace.clone(),
                    entropy: self.entropy.clone(),
                    now,
                }))
            }
        }
    }

    /// One pipeline over `catalog`, with the snapshot store when there is one.
    fn staged<C: BundleCatalog>(&self, catalog: C) -> ImportService<C> {
        match &self.store {
            Some(store) => ImportService::new(Arc::clone(store), catalog),
            None => ImportService::without_store(catalog),
        }
    }
}

/// Which pipeline a destination resolved to.
#[derive(Debug)]
enum Service {
    /// Landing in the operator-curated catalogue.
    Platform(ImportService<PlatformCatalog>),
    /// Landing in one workspace's own library.
    Workspace(ImportService<tenant::TenantCatalog>),
}

#[derive(Debug)]
struct PlatformCatalog {
    database: Db,
    replace: bool,
    now: UnixMillis,
}

impl BundleCatalog for PlatformCatalog {
    async fn insert(&self, body: &ImportBody, bundle: &PreparedBundle) -> Result<String> {
        let requirements = serde_json::to_string(&bundle.requirements)?;
        let support_files = serde_json::to_string(&bundle.support_manifest)?;
        let skill = markdown("SKILL.md", &body.skill_markdown)?;
        let trigger = body
            .trigger_markdown
            .as_deref()
            .map(|value| markdown("TRIGGER.md", value))
            .transpose()?;
        let mut connection = self.database.acquire().await?;
        let inserted: Option<String> = sqlx::query_scalar(UPSERT)
            .bind(&bundle.name)
            .bind(&bundle.name)
            .bind(&bundle.description)
            .bind(&body.source_ref)
            .bind(body.source_revision.as_deref().unwrap_or(DEFAULT_REVISION))
            .bind(requirements)
            .bind(VISIBILITY_DRAFT)
            .bind(&bundle.content_hash)
            .bind(skill)
            .bind(trigger)
            .bind(support_files)
            .bind(self.now.as_millis())
            .bind(self.replace)
            .fetch_optional(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_IMPORT))?;
        // The slug this catalogue is keyed by, echoed from the statement rather
        // than re-derived from the bundle: the two agree here and do not on the
        // tenant tier, and a caller should not have to know which.
        if let Some(id) = inserted {
            return Ok(id);
        }
        let incumbent = sqlx::query(COLLISION_OWNER)
            .bind(&bundle.name)
            .fetch_one(&mut *connection)
            .await
            .map_err(Error::database(CONTEXT_COLLISION))?
            .try_get(0)
            .map_err(Error::database(CONTEXT_COLLISION))?;
        Err(Error::catalog_id_collision(incumbent))
    }
}

pub(super) fn markdown<'a>(document: &'static str, value: &'a [u8]) -> Result<&'a str> {
    core::str::from_utf8(value).map_err(|source| Error::FrontmatterUtf8 { document, source })
}

const UPSERT: &str = "INSERT INTO core.fleet_library (id,name,description,source_repo,source_path,source_ref,required_credentials,required_credentials_reasons,required_tools,network_hosts,visibility,content_hash,skill_markdown,trigger_markdown,support_files_json,created_at,updated_at) VALUES ($1,$2,$3,$4,'',$5,($6::jsonb->'credentials'),'{}'::jsonb,($6::jsonb->'tools'),($6::jsonb->'network_hosts'),$7,$8,$9,$10,$11::jsonb,$12,$12) ON CONFLICT (id) DO UPDATE SET source_repo=EXCLUDED.source_repo,source_ref=EXCLUDED.source_ref,required_credentials=EXCLUDED.required_credentials,required_credentials_reasons=CASE WHEN jsonb_array_length(EXCLUDED.required_credentials)=0 THEN core.fleet_library.required_credentials_reasons ELSE (SELECT COALESCE(jsonb_object_agg(k,v),'{}'::jsonb) FROM jsonb_each_text(core.fleet_library.required_credentials_reasons) AS r(k,v) WHERE r.k IN (SELECT jsonb_array_elements_text(EXCLUDED.required_credentials))) END,required_tools=EXCLUDED.required_tools,network_hosts=EXCLUDED.network_hosts,visibility=EXCLUDED.visibility,content_hash=EXCLUDED.content_hash,skill_markdown=EXCLUDED.skill_markdown,trigger_markdown=EXCLUDED.trigger_markdown,support_files_json=EXCLUDED.support_files_json,updated_at=EXCLUDED.updated_at WHERE $13::boolean OR core.fleet_library.source_repo=EXCLUDED.source_repo RETURNING id";
const COLLISION_OWNER: &str = "SELECT source_repo FROM core.fleet_library WHERE id=$1";
