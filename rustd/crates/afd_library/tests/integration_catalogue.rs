//! Fleet-library catalogue proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::sync::atomic::{AtomicU32, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};
use afd_db::{Db, Migrator};
use afd_library::{DeleteLibrary, Libraries, LibraryPatch, PatchLibrary};
use afd_library::{ImportBody, LibraryImports, SourceKind};
use sqlx::AssertSqlSafe;

const LANE_KNOB: &str = "TEST_DATABASE_URL";
const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

const SEED: &str = r#"
INSERT INTO core.fleet_library (
    id, name, description, source_repo, source_path, source_ref,
    required_credentials, required_credentials_reasons, required_tools,
    network_hosts, visibility, content_hash, skill_markdown, trigger_markdown,
    support_files_json, created_at, updated_at
) VALUES
    ('draft-fleet', 'Draft Fleet', 'draft description', 'agentsfleet/draft-fleet', '', 'main',
     '["github"]', '{}', '["git"]', '["api.github.com"]', 'draft', NULL,
     NULL, NULL, NULL, 1, 1),
    ('public-fleet', 'Public Fleet', 'public description', 'agentsfleet/public-fleet', '', 'v1',
     '[]', '{}', '[]', '[]', 'public', '0123456789abcdef',
     '# Public Fleet', NULL, '[]', 2, 2)
"#;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn catalogue_edits_preserve_preconditions_and_publication_invariants() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let libraries = Libraries::new(fixtures.database.clone());

    let entries = libraries.list().await.expect("the catalogue lists");
    assert_eq!(entries.len(), 2);
    let draft = entries
        .iter()
        .find(|entry| entry.id() == "draft-fleet")
        .expect("the draft fixture lists");
    assert_eq!(draft.requirements().credentials(), &["github"]);
    assert_eq!(draft.requirements().tools(), &["git"]);
    assert_eq!(draft.requirements().network_hosts(), &["api.github.com"]);
    assert!(!draft.requirements().trigger_present());
    let draft_etag = draft.etag().to_owned();

    assert_eq!(
        libraries
            .patch("draft-fleet", &published(), None, NOW)
            .await
            .expect("publication is a typed outcome"),
        PatchLibrary::PublishWithoutBundle
    );
    assert_eq!(
        libraries
            .patch(
                "draft-fleet",
                &renamed("Changed despite stale tag"),
                Some("\"stale\""),
                NOW,
            )
            .await
            .expect("a stale write is a typed outcome"),
        PatchLibrary::Stale { etag: draft_etag }
    );
    assert_eq!(
        libraries
            .list()
            .await
            .expect("the catalogue still lists")
            .into_iter()
            .find(|entry| entry.id() == "draft-fleet")
            .expect("the draft remains")
            .name(),
        "Draft Fleet"
    );

    assert_eq!(
        libraries
            .delete("public-fleet")
            .await
            .expect("public deletion is a typed outcome"),
        DeleteLibrary::Published
    );
    let public_etag = libraries
        .list()
        .await
        .expect("the catalogue lists before source edit")
        .into_iter()
        .find(|entry| entry.id() == "public-fleet")
        .expect("the public fixture remains")
        .etag()
        .to_owned();
    let PatchLibrary::Updated(updated) = libraries
        .patch("public-fleet", &source_ref("v2"), Some(&public_etag), NOW)
        .await
        .expect("the source edit succeeds")
    else {
        panic!("the matching edit must update");
    };
    assert_eq!(updated.visibility(), "draft");
    assert_eq!(updated.content_hash(), None);
    assert_eq!(updated.source_ref(), "v2");

    assert_eq!(
        libraries
            .delete("public-fleet")
            .await
            .expect("the withdrawn row deletes"),
        DeleteLibrary::Deleted
    );
    assert_eq!(
        libraries
            .delete("missing-fleet")
            .await
            .expect("a missing row is a typed outcome"),
        DeleteLibrary::NotFound
    );

    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_upload_stages_draft_and_guards_source_ownership() {
    let fixtures = Fixtures::create().await;
    let imports = LibraryImports::without_store(fixtures.database.clone());
    let libraries = Libraries::new(fixtures.database.clone());

    let first = upload("unit/first", "First body");
    let imported = imports
        .upload(&first, false, NOW)
        .await
        .expect("a skill-only upload needs no R2");
    assert_eq!(imported.name, "upload-probe");
    assert!(
        libraries
            .published()
            .await
            .expect("published list reads")
            .is_empty()
    );
    let row = libraries
        .list()
        .await
        .expect("admin catalogue reads")
        .into_iter()
        .next()
        .expect("the upload inserted one row");
    assert_eq!(row.visibility(), "draft");
    assert_eq!(row.source_repo(), "unit/first");

    let second = upload("unit/second", "Second body");
    let collision = imports
        .upload(&second, false, NOW)
        .await
        .expect_err("a foreign source cannot take the slug silently");
    assert_eq!(collision.collision_incumbent(), Some("unit/first"));
    imports
        .upload(&second, true, NOW)
        .await
        .expect("explicit replacement changes the source");

    assert!(matches!(
        libraries
            .patch("upload-probe", &published(), None, NOW)
            .await
            .expect("the imported bundle publishes"),
        PatchLibrary::Updated(_)
    ));
    let public = libraries.published().await.expect("published list reads");
    assert_eq!(public.len(), 1);
    assert_eq!(
        public.first().expect("one published row").id(),
        "upload-probe"
    );

    fixtures.cleanup().await;
}

fn upload(source_ref: &str, body: &str) -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: source_ref.to_owned(),
        source_revision: None,
        skill_markdown: format!(
            "---\nname: upload-probe\ndescription: {body}\nversion: 1.0.0\n---\nInstructions."
        )
        .into_bytes(),
        trigger_markdown: None,
        support_files: Vec::new(),
    }
}

fn published() -> LibraryPatch {
    LibraryPatch::new(None, None, None, None, None, Some(true))
}

fn renamed(name: &str) -> LibraryPatch {
    LibraryPatch::new(Some(name.to_owned()), None, None, None, None, None)
}

fn source_ref(revision: &str) -> LibraryPatch {
    LibraryPatch::new(None, None, None, Some(revision.to_owned()), None, None)
}

struct Fixtures {
    base_url: String,
    name: String,
    database: Db,
}

impl Fixtures {
    async fn create() -> Self {
        let base_url = std::env::var(LANE_KNOB).unwrap_or_else(|_error| {
            panic!("{LANE_KNOB} is unset — run through `make test-integration-rustd`")
        });
        let name = format!(
            "afd_library_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        admin(&base_url, AssertSqlSafe(format!("CREATE DATABASE {name}"))).await;
        let url = database_url(&base_url, &name);
        let migrator = open(&url, DbRole::Migrator).await;
        Migrator::new()
            .run(&migrator)
            .await
            .expect("the schema applies");
        drop(migrator);
        Self {
            database: open(&url, DbRole::Api).await,
            base_url,
            name,
        }
    }

    async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .execute(&mut *connection)
            .await
            .expect("the catalogue fixture seeds");
    }

    async fn cleanup(self) {
        drop(self.database);
        admin(
            &self.base_url,
            AssertSqlSafe(format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            )),
        )
        .await;
    }
}

fn database_url(base_url: &str, name: &str) -> String {
    let (prefix, tail) = base_url
        .rsplit_once('/')
        .expect("a Postgres URL has a database path");
    let query = tail.split_once('?').map_or("", |(_, query)| query);
    if query.is_empty() {
        format!("{prefix}/{name}")
    } else {
        format!("{prefix}/{name}?{query}")
    }
}

async fn open(url: &str, role: DbRole) -> Db {
    let env = MapEnv::from_pairs(DbRole::ALL.iter().map(|each| (each.url_knob(), url)));
    Db::connect(&PoolConfig::resolve(&env, role).expect("the URL resolves"))
        .await
        .expect("the database accepts a connection")
}

async fn admin(base_url: &str, statement: AssertSqlSafe<String>) {
    let pool = sqlx::PgPool::connect(base_url)
        .await
        .expect("the lane database is reachable");
    let mut connection = pool.acquire().await.expect("an admin connection");
    sqlx::query(statement)
        .execute(&mut *connection)
        .await
        .expect("the admin statement runs");
    drop(connection);
    pool.close().await;
}
