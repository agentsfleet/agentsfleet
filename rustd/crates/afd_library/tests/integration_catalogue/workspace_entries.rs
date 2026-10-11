//! A workspace's own entries: onboarding the same bytes again, removing from
//! another workspace, and the platform listing a tenant installs from.
//!
//! Every statement here is scoped by its arguments — a workspace, a content
//! hash, a publication state — and each test hands it an argument that a
//! statement which had lost that scope would answer for anyway.

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::{Destination, Libraries, LibraryImports, OwnedEntry};

use super::fixtures::NOW;
use super::gallery_seed::{PUBLISHED, parse, seed_platform, seed_platform_as, seed_scope};
use super::workspace_gallery::{archive, serve};

/// A second onboarding's instant, later than the first.
const LATER: UnixMillis = UnixMillis::from_millis(NOW.as_millis() + 1_000);

/// What a page asks for when it wants the whole collection.
const WHOLE_PAGE: u32 = 50;

/// The repository every onboarding here names; the local origin answers it.
const REPOSITORY: &str = "agentsfleet/reviewer";

/// The revision every onboarding here names.
const REVISION: &str = "main";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn onboarding_the_same_bytes_again_answers_the_first_row_in_each_workspace() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let (mine, theirs) = (mint_id(), mint_id());
    seed_scope(&database, &mine).await;
    seed_scope(&database, &theirs).await;
    let name = format!("twice-{}", mint_id().replace('-', ""));

    let first = onboard(&database, &name, &mine, NOW).await;
    let elsewhere = onboard(&database, &name, &theirs, NOW).await;
    assert_ne!(first, elsewhere, "one bundle in two workspaces is two rows");

    assert_eq!(onboard(&database, &name, &mine, LATER).await, first);
    assert_eq!(onboard(&database, &name, &theirs, LATER).await, elsewhere);
    let [held] = <[OwnedEntry; 1]>::try_from(owned(&database, &mine, &name).await)
        .expect("a re-onboard writes no second row");
    assert_eq!(held.id, first);
    assert_eq!(
        held.created_at_ms,
        NOW.as_millis(),
        "a re-onboard of the same bytes changes nothing"
    );

    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_workspace_removes_only_its_own_entries() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let (mine, theirs) = (mint_id(), mint_id());
    seed_scope(&database, &mine).await;
    seed_scope(&database, &theirs).await;
    let name = format!("kept-{}", mint_id().replace('-', ""));
    let entry = onboard(&database, &name, &mine, NOW).await;
    let libraries = Libraries::new(database.clone());

    assert!(
        owned(&database, &theirs, &name).await.is_empty(),
        "another workspace's collection does not list the entry"
    );
    assert!(
        !libraries
            .remove_entry(&parse(&theirs), &parse(&entry))
            .await
            .expect("a foreign removal is an answer"),
        "another workspace removes nothing"
    );
    assert_eq!(owned(&database, &mine, &name).await.len(), 1);
    assert!(
        libraries
            .remove_entry(&parse(&mine), &parse(&entry))
            .await
            .expect("the owner's removal is an answer"),
        "the owner removes its entry"
    );

    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_public_listing_shows_only_what_a_tenant_could_install() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let suffix = mint_id().replace('-', "");
    let installable = format!("listed-{suffix}");
    let empty = format!("unlisted-{suffix}");
    seed_platform(&database, &installable, NOW.as_millis()).await;
    seed_platform_as(&database, &empty, NOW.as_millis(), PUBLISHED, None).await;

    let listed: Vec<String> = Libraries::new(database.clone())
        .published()
        .await
        .expect("the public listing reads")
        .iter()
        .map(|item| item.id().to_owned())
        .filter(|id| id.ends_with(&suffix))
        .collect();
    assert_eq!(
        listed,
        [installable],
        "a published row with no snapshot is not listed"
    );

    drop(database);
    lane.cleanup().await;
}

/// Onboards the one-file bundle `name` into `workspace`, answering the row id.
async fn onboard(database: &afd_db::Db, name: &str, workspace: &str, now: UnixMillis) -> String {
    LibraryImports::without_store(database.clone(), Entropy::new())
        .with_github_api_base(serve(&archive(name)))
        .github(
            REPOSITORY,
            Some(REVISION),
            Destination::Workspace(&parse(workspace)),
            now,
        )
        .await
        .expect("the onboarding completes through the local origin")
        .id
}

/// `workspace`'s own entries named `name`.
async fn owned(database: &afd_db::Db, workspace: &str, name: &str) -> Vec<OwnedEntry> {
    Libraries::new(database.clone())
        .owned_entries(&parse(workspace), WHOLE_PAGE, None)
        .await
        .expect("the owned collection reads")
        .items
        .into_iter()
        .filter(|entry| entry.name == name)
        .collect()
}
