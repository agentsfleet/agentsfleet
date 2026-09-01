//! The catalogue lane's seeded rows, frozen clock, and patch builders.
//!
//! Split from `integration_catalogue.rs` at the file cap, along the line
//! between what the lane PROVES and what it stands on. Both tests there build
//! the same two library rows and edit them through the same small patches, so
//! the fixture is shared by construction rather than by copy — and a schema
//! column moving lands in one file instead of two.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_db::Db;
use afd_db::config::DbRole;
use afd_db::test_util::TestDatabase;
use afd_library::{ImportBody, LibraryPatch, SourceKind};

pub(super) const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

const SEED: &str = r#"
INSERT INTO core.fleet_library (
    id, name, description, source_repo, source_path, source_ref,
    required_credentials, required_credentials_reasons, required_tools,
    network_hosts, visibility, content_hash, skill_markdown, trigger_markdown,
    support_files_json, created_at, updated_at
) VALUES
    ($1, 'Draft Fleet', 'draft description', $3, '', 'main',
     '["github"]', '{}', '["git"]', '["api.github.com"]', 'draft', NULL,
     NULL, NULL, NULL, 1, 1),
    ($2, 'Public Fleet', 'public description', $4, '', 'v1',
     '[]', '{}', '[]', '[]', 'public', '0123456789abcdef',
     '# Public Fleet', NULL, '[]', 2, 2)
"#;

pub(super) fn upload(slug: &str, source_ref: &str, body: &str) -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: source_ref.to_owned(),
        source_revision: None,
        skill_markdown: format!(
            "---\nname: {slug}\ndescription: {body}\nversion: 1.0.0\n---\nInstructions."
        )
        .into_bytes(),
        trigger_markdown: None,
        support_files: Vec::new(),
    }
}

pub(super) fn published() -> LibraryPatch {
    LibraryPatch::new(None, None, None, None, None, Some(true))
}

pub(super) fn renamed(name: &str) -> LibraryPatch {
    LibraryPatch::new(Some(name.to_owned()), None, None, None, None, None)
}

pub(super) fn source_ref(revision: &str) -> LibraryPatch {
    LibraryPatch::new(None, None, None, Some(revision.to_owned()), None, None)
}

pub(super) struct Fixtures {
    lane: TestDatabase,
    pub(super) database: Db,
    pub(super) suffix: String,
    pub(super) draft_id: String,
    pub(super) public_id: String,
    pub(super) upload_id: String,
}

impl Fixtures {
    pub(super) async fn create() -> Self {
        let lane = TestDatabase::shared();
        let suffix = afd_db::test_util::mint_id().replace('-', "");
        Self {
            database: lane.open(DbRole::Api, &[]).await,
            draft_id: format!("draft-{suffix}"),
            public_id: format!("public-{suffix}"),
            upload_id: format!("upload-{suffix}"),
            suffix,
            lane,
        }
    }

    pub(super) async fn seed(&self) {
        let mut connection = self.database.acquire().await.expect("an API connection");
        sqlx::query(SEED)
            .bind(&self.draft_id)
            .bind(&self.public_id)
            .bind(format!("agentsfleet/{}", self.draft_id))
            .bind(format!("agentsfleet/{}", self.public_id))
            .execute(&mut *connection)
            .await
            .expect("the catalogue fixture seeds");
    }

    pub(super) async fn cleanup(self) {
        drop(self.database);
        self.lane.cleanup().await;
    }
}
