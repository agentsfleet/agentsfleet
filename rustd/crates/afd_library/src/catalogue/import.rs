//! Platform onboarding over validated bundle inputs.

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_db::Db;
use object_store::ObjectStore;
use sqlx::Row as _;

use super::VISIBILITY_DRAFT;
use crate::{
    BundleCatalog, BundleSource, Error, GithubSource, ImportBody, ImportService, PreparedBundle,
    Result, SourceKind,
};

const CONTEXT_IMPORT: &str = "import platform Fleet Bundle";
const CONTEXT_COLLISION: &str = "read Fleet Bundle collision owner";
const DEFAULT_REVISION: &str = "main";
const TEMPLATE_OWNER: &str = "agentsfleet";

/// Platform importer sharing the daemon's one snapshot-store handle.
#[derive(Debug, Clone)]
pub struct LibraryImports {
    database: Db,
    store: Option<Arc<dyn ObjectStore>>,
}

impl LibraryImports {
    /// Uses a configured snapshot store for bundles carrying support files.
    #[must_use]
    pub fn new(database: Db, store: Arc<dyn ObjectStore>) -> Self {
        Self {
            database,
            store: Some(store),
        }
    }

    /// Keeps skill-only onboarding available when snapshot storage is absent.
    #[must_use]
    pub const fn without_store(database: Db) -> Self {
        Self {
            database,
            store: None,
        }
    }

    /// Validates and persists an inline upload.
    ///
    /// # Errors
    /// Reports validation, storage, collision, or database failures.
    pub async fn upload(
        &self,
        body: &ImportBody,
        replace: bool,
        now: UnixMillis,
    ) -> Result<PreparedBundle> {
        self.persist(body, replace, now).await
    }

    /// Fetches a public GitHub repository, then validates and persists it.
    ///
    /// # Errors
    /// Reports source, validation, storage, collision, or database failures.
    pub async fn github(
        &self,
        repository: &str,
        revision: Option<&str>,
        replace: bool,
        now: UnixMillis,
    ) -> Result<PreparedBundle> {
        let source = GithubSource::new(revision.unwrap_or(DEFAULT_REVISION))?;
        let body = source.fetch(repository).await?;
        self.persist(&body, replace, now).await
    }

    /// Fetches one first-party template from its fixed GitHub repository.
    ///
    /// # Errors
    /// Reports source, validation, storage, collision, or database failures.
    pub async fn template(
        &self,
        template: &str,
        replace: bool,
        now: UnixMillis,
    ) -> Result<PreparedBundle> {
        let repository = format!("{TEMPLATE_OWNER}/{template}");
        let source = GithubSource::new(DEFAULT_REVISION)?;
        let mut body = source.fetch(&repository).await?;
        body.source_kind = SourceKind::Template;
        body.source_ref = template.to_owned();
        body.source_revision = None;
        self.persist(&body, replace, now).await
    }

    async fn persist(
        &self,
        body: &ImportBody,
        replace: bool,
        now: UnixMillis,
    ) -> Result<PreparedBundle> {
        let catalog = PlatformCatalog {
            database: self.database.clone(),
            replace,
            now,
        };
        match &self.store {
            Some(store) => {
                ImportService::new(Arc::clone(store), catalog)
                    .import(body)
                    .await
            }
            None => ImportService::without_store(catalog).import(body).await,
        }
    }
}

#[derive(Debug)]
struct PlatformCatalog {
    database: Db,
    replace: bool,
    now: UnixMillis,
}

impl BundleCatalog for PlatformCatalog {
    async fn insert(&self, body: &ImportBody, bundle: &PreparedBundle) -> Result<()> {
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
        if inserted.is_some() {
            return Ok(());
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

fn markdown<'a>(document: &'static str, value: &'a [u8]) -> Result<&'a str> {
    core::str::from_utf8(value).map_err(|source| Error::FrontmatterUtf8 { document, source })
}

const UPSERT: &str = "INSERT INTO core.fleet_library (id,name,description,source_repo,source_path,source_ref,required_credentials,required_credentials_reasons,required_tools,network_hosts,visibility,content_hash,skill_markdown,trigger_markdown,support_files_json,created_at,updated_at) VALUES ($1,$2,$3,$4,'',$5,($6::jsonb->'credentials'),'{}'::jsonb,($6::jsonb->'tools'),($6::jsonb->'network_hosts'),$7,$8,$9,$10,$11::jsonb,$12,$12) ON CONFLICT (id) DO UPDATE SET source_repo=EXCLUDED.source_repo,source_ref=EXCLUDED.source_ref,required_credentials=EXCLUDED.required_credentials,required_credentials_reasons=CASE WHEN jsonb_array_length(EXCLUDED.required_credentials)=0 THEN core.fleet_library.required_credentials_reasons ELSE (SELECT COALESCE(jsonb_object_agg(k,v),'{}'::jsonb) FROM jsonb_each_text(core.fleet_library.required_credentials_reasons) AS r(k,v) WHERE r.k IN (SELECT jsonb_array_elements_text(EXCLUDED.required_credentials))) END,required_tools=EXCLUDED.required_tools,network_hosts=EXCLUDED.network_hosts,visibility=EXCLUDED.visibility,content_hash=EXCLUDED.content_hash,skill_markdown=EXCLUDED.skill_markdown,trigger_markdown=EXCLUDED.trigger_markdown,support_files_json=EXCLUDED.support_files_json,updated_at=EXCLUDED.updated_at WHERE $13::boolean OR core.fleet_library.source_repo=EXCLUDED.source_repo RETURNING id";
const COLLISION_OWNER: &str = "SELECT source_repo FROM core.fleet_library WHERE id=$1";
