//! Fleet-library catalogue proofs against the migrated Postgres schema.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_library::{DeleteLibrary, Destination, Libraries, PatchLibrary};
use std::sync::Arc;

use afd_library::{LibraryImports, SupportFile};
use object_store::memory::InMemory;

#[path = "integration_catalogue/fixtures.rs"]
mod fixtures;
#[path = "integration_catalogue/source_imports.rs"]
mod source_imports;
#[path = "integration_catalogue/workspace_gallery.rs"]
mod workspace_gallery;

use self::fixtures::{Fixtures, NOW, published, renamed, source_ref, upload};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn catalogue_edits_preserve_preconditions_and_publication_invariants() {
    let fixtures = Fixtures::create().await;
    fixtures.seed().await;
    let libraries = Libraries::new(fixtures.database.clone());

    let draft_etag = assert_seeded_requirements(&libraries, &fixtures).await;
    assert_publication_preconditions(&libraries, &fixtures, &draft_etag).await;
    withdraw_and_edit_public(&libraries, &fixtures).await;
    assert_deletion_outcomes(&libraries, &fixtures).await;
    fixtures.cleanup().await;
}

async fn assert_seeded_requirements(libraries: &Libraries, fixtures: &Fixtures) -> String {
    let entries = libraries.list().await.expect("the catalogue lists");
    let draft = entries
        .iter()
        .find(|entry| entry.id() == fixtures.draft_id)
        .expect("the draft fixture lists");
    assert_eq!(draft.requirements().credentials(), &["github"]);
    assert_eq!(draft.requirements().tools(), &["git"]);
    assert_eq!(draft.requirements().network_hosts(), &["api.github.com"]);
    assert!(!draft.requirements().trigger_present());
    draft.etag().to_owned()
}

async fn assert_publication_preconditions(
    libraries: &Libraries,
    fixtures: &Fixtures,
    draft_etag: &str,
) {
    assert_eq!(
        libraries
            .patch(&fixtures.draft_id, &published(), None, NOW)
            .await
            .expect("publication is a typed outcome"),
        PatchLibrary::PublishWithoutBundle
    );
    assert_eq!(
        libraries
            .patch(
                &fixtures.draft_id,
                &renamed("Changed despite stale tag"),
                Some("\"stale\""),
                NOW,
            )
            .await
            .expect("a stale write is a typed outcome"),
        PatchLibrary::Stale {
            etag: draft_etag.to_owned()
        }
    );
    assert_eq!(
        libraries
            .list()
            .await
            .expect("the catalogue still lists")
            .into_iter()
            .find(|entry| entry.id() == fixtures.draft_id)
            .expect("the draft remains")
            .name(),
        "Draft Fleet"
    );
}

async fn withdraw_and_edit_public(libraries: &Libraries, fixtures: &Fixtures) {
    assert_eq!(
        libraries
            .delete(&fixtures.public_id)
            .await
            .expect("public deletion is a typed outcome"),
        DeleteLibrary::Published
    );
    let public_etag = libraries
        .list()
        .await
        .expect("the catalogue lists before source edit")
        .into_iter()
        .find(|entry| entry.id() == fixtures.public_id)
        .expect("the public fixture remains")
        .etag()
        .to_owned();
    let PatchLibrary::Updated(updated) = libraries
        .patch(
            &fixtures.public_id,
            &source_ref("v2"),
            Some(&public_etag),
            NOW,
        )
        .await
        .expect("the source edit succeeds")
    else {
        panic!("the matching edit must update");
    };
    assert_eq!(updated.visibility(), "draft");
    assert_eq!(updated.content_hash(), None);
    assert_eq!(updated.source_ref(), "v2");
}

async fn assert_deletion_outcomes(libraries: &Libraries, fixtures: &Fixtures) {
    assert_eq!(
        libraries
            .delete(&fixtures.public_id)
            .await
            .expect("the withdrawn row deletes"),
        DeleteLibrary::Deleted
    );
    assert_eq!(
        libraries
            .delete(&format!("missing-{}", fixtures.suffix))
            .await
            .expect("a missing row is a typed outcome"),
        DeleteLibrary::NotFound
    );
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn platform_upload_stages_draft_and_guards_source_ownership() {
    let fixtures = Fixtures::create().await;
    let imports = LibraryImports::without_store(fixtures.database.clone(), Entropy::new());
    let libraries = Libraries::new(fixtures.database.clone());

    let first_source = upload_initial_draft(&imports, &libraries, &fixtures).await;
    replace_foreign_source(&imports, &fixtures, &first_source).await;
    publish_uploaded_draft(&libraries, &fixtures).await;
    store_support_snapshot(&fixtures).await;
    assert_invalid_imports(&imports, &fixtures).await;
    fixtures.cleanup().await;
}

async fn upload_initial_draft(
    imports: &LibraryImports,
    libraries: &Libraries,
    fixtures: &Fixtures,
) -> String {
    let first_source = format!("unit/{}/first", fixtures.suffix);
    let first = upload(&fixtures.upload_id, &first_source, "First body");
    let imported = imports
        .upload(&first, Destination::Platform { replace: false }, NOW)
        .await
        .expect("a skill-only upload needs no R2");
    assert_eq!(imported.bundle.name, fixtures.upload_id);
    assert!(
        libraries
            .published()
            .await
            .expect("published list reads")
            .iter()
            .all(|entry| entry.id() != fixtures.upload_id)
    );
    let row = libraries
        .list()
        .await
        .expect("admin catalogue reads")
        .into_iter()
        .find(|entry| entry.id() == fixtures.upload_id)
        .expect("the upload inserted one row");
    assert_eq!(row.visibility(), "draft");
    assert_eq!(row.source_repo(), first_source);
    first_source
}

async fn replace_foreign_source(imports: &LibraryImports, fixtures: &Fixtures, first_source: &str) {
    let second_source = format!("unit/{}/second", fixtures.suffix);
    let second = upload(&fixtures.upload_id, &second_source, "Second body");
    let collision = imports
        .upload(&second, Destination::Platform { replace: false }, NOW)
        .await
        .expect_err("a foreign source cannot take the slug silently");
    assert_eq!(collision.collision_incumbent(), Some(first_source));
    imports
        .upload(&second, Destination::Platform { replace: true }, NOW)
        .await
        .expect("explicit replacement changes the source");
}

async fn publish_uploaded_draft(libraries: &Libraries, fixtures: &Fixtures) {
    assert!(matches!(
        libraries
            .patch(&fixtures.upload_id, &published(), None, NOW)
            .await
            .expect("the imported bundle publishes"),
        PatchLibrary::Updated(_)
    ));
    let public = libraries.published().await.expect("published list reads");
    assert_eq!(
        public
            .iter()
            .find(|entry| entry.id() == fixtures.upload_id)
            .expect("the uploaded row publishes")
            .id(),
        fixtures.upload_id
    );
}

async fn store_support_snapshot(fixtures: &Fixtures) {
    let stored_id = format!("stored-{}", fixtures.suffix);
    let mut stored = upload(
        &stored_id,
        &format!("unit/{}/stored", fixtures.suffix),
        "Stored support",
    );
    stored.support_files.push(SupportFile {
        path: "notes/context.txt".to_owned(),
        content: b"context".to_vec(),
    });
    let stored = LibraryImports::new(
        fixtures.database.clone(),
        Arc::new(InMemory::new()),
        Entropy::new(),
    )
    .upload(&stored, Destination::Platform { replace: false }, NOW)
    .await
    .expect("a configured store accepts the canonical snapshot");
    assert_eq!(stored.bundle.support_manifest.len(), 1);
}

async fn assert_invalid_imports(imports: &LibraryImports, fixtures: &Fixtures) {
    let mut invalid_utf8 = upload(
        &format!("invalid-{}", fixtures.suffix),
        &format!("unit/{}/invalid", fixtures.suffix),
        "unused",
    );
    invalid_utf8.skill_markdown = vec![0xff];
    let error = imports
        .upload(&invalid_utf8, Destination::Platform { replace: false }, NOW)
        .await
        .expect_err("non-UTF-8 root documents are refused before persistence");
    assert!(error.to_string().contains("SKILL.md is not UTF-8"));

    let unsafe_revision = imports
        .github(
            "agentsfleet/reviewer",
            Some("bad/ref"),
            Destination::Platform { replace: false },
            NOW,
        )
        .await
        .expect_err("an unsafe revision is refused before a network request");
    assert!(unsafe_revision.to_string().contains("repository reference"));

    let unsafe_template = imports
        .template("nested/name", Destination::Platform { replace: false }, NOW)
        .await
        .expect_err("a template cannot escape the fixed first-party owner");
    assert!(unsafe_template.to_string().contains("repository reference"));
}
