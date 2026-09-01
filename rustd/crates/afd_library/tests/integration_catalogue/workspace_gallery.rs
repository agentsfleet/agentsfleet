//! The workspace gallery's merged order and seek, and the onboarding round trip.
//!
//! The three outcomes the router-tier suite next door cannot reach: `GET` and
//! `POST` on `/v1/workspaces/{id}/fleet-libraries` both open with a statement a
//! real Postgres evaluates, so over an unreachable pool each renders as the
//! same 503. What that proves is the guard, the scope rung, the ownership layer
//! and the cursor's binding — everything in FRONT of the verbs. The merge
//! itself, the keyset predicate that walks it, and the write an onboarding
//! leaves behind are the store's answers and are graded here.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use afd_core::clock::UnixMillis;
use afd_crypto::entropy::Entropy;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::{Destination, Libraries, LibraryImports, Tier};
use flate2::Compression;
use flate2::write::GzEncoder;

/// The instant the fixture rows are stamped with.
const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

/// What a page asks for when the test wants every seeded row at once.
const WHOLE_PAGE: u32 = 50;

/// The four fixture instants, newest to oldest.
///
/// Named rather than spelled inline because the LADDER is the subject: the two
/// libraries alternate down it, so a merge that concatenated one after the
/// other would still return four cards and could only be caught by their
/// sequence. The values are small and far from any real clock, which keeps
/// these rows at the old end of a gallery the lane shares with other tests.
const FIRST_INSTANT: i64 = 4_000;
const SECOND_INSTANT: i64 = 3_000;
const THIRD_INSTANT: i64 = 2_000;
const FOURTH_INSTANT: i64 = 1_000;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_gallery_merges_two_libraries_under_one_order_and_walks_it_by_seek() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let workspace = mint_id();
    seed_scope(&database, &workspace).await;
    let libraries = Libraries::new(database.clone());
    let suffix = mint_id().replace('-', "");

    // Two rows in each library, interleaved in TIME so the merge cannot be
    // faked by concatenating one library after the other: the order is
    // (created_at DESC, tier, id), and a naive UNION that forgot the ORDER BY
    // would still return all four.
    seed_platform(&database, &format!("plat-a-{suffix}"), FIRST_INSTANT).await;
    seed_workspace(
        &database,
        &workspace,
        &format!("tenant-a-{suffix}"),
        SECOND_INSTANT,
    )
    .await;
    seed_platform(&database, &format!("plat-b-{suffix}"), THIRD_INSTANT).await;
    seed_workspace(
        &database,
        &workspace,
        &format!("tenant-b-{suffix}"),
        FOURTH_INSTANT,
    )
    .await;

    let whole = libraries
        .gallery(&parse(&workspace), WHOLE_PAGE, None)
        .await
        .expect("the gallery reads");
    // Matched on NAME, not id: the platform arm projects a TEXT slug and the
    // tenant arm projects `id::text` from a UUID column, so an id filter would
    // silently see the platform rows only and call a half-merge a merge.
    let seeded: Vec<_> = whole
        .items
        .iter()
        .filter(|card| card.name.ends_with(&suffix))
        .collect();
    assert_eq!(seeded.len(), 4, "both libraries are in one page");
    let stamps: Vec<_> = seeded.iter().map(|card| card.created_at_ms).collect();
    let mut newest_first = stamps.clone();
    newest_first.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        stamps, newest_first,
        "newest first, across BOTH libraries rather than within each"
    );

    let tiers: Vec<_> = seeded.iter().map(|card| card.tier).collect();
    assert_eq!(
        tiers,
        vec![Tier::Platform, Tier::Tenant, Tier::Platform, Tier::Tenant],
        "the merge interleaves the two libraries rather than concatenating them"
    );

    // The seek: one card at a time, resuming from the boundary the previous
    // page handed back. A predicate that compared on the wrong column, or
    // compared non-strictly, shows up here as a repeat or a skip.
    // Walked to EXHAUSTION rather than for a fixed number of pages: the lane
    // shares one database, so the gallery carries other tests' rows too and a
    // fixed bound would stop partway and read as a seek defect. The cap is a
    // deadlock guard — a predicate that failed to advance would otherwise spin
    // here rather than fail.
    let mut walked = Vec::new();
    let mut after = None;
    for _page in 0..500 {
        let page = libraries
            .gallery(&parse(&workspace), 1, after.as_ref())
            .await
            .expect("the gallery reads");
        let Some(card) = page.items.first() else {
            break;
        };
        if card.name.ends_with(&suffix) {
            walked.push(card.name.clone());
        }
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let expected: Vec<_> = seeded.iter().map(|card| card.name.clone()).collect();
    assert_eq!(
        walked, expected,
        "the seek walks the same order the whole page reports, once each"
    );

    drop(database);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_onboarding_lands_in_the_workspaces_own_library_and_nobody_elses() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let workspace = mint_id();
    let neighbour = mint_id();
    seed_scope(&database, &workspace).await;
    seed_scope(&database, &neighbour).await;
    let name = format!("onboarded-{}", mint_id().replace('-', ""));

    // The bundle is fetched over a local origin rather than GitHub: what this
    // grades is the WRITE the onboarding leaves behind, and a test that needed
    // the network to reach it would be grading the network too.
    let imports = LibraryImports::without_store(database.clone(), Entropy::new())
        .with_github_api_base(serve(&archive(&name)));

    let onboarded = imports
        .github(
            "agentsfleet/reviewer",
            Some("main"),
            Destination::Workspace(&parse(&workspace)),
            NOW,
        )
        .await
        .expect("the onboarding completes through the local origin");
    assert_eq!(onboarded.bundle.name, name);

    // The round trip: what was written is what the gallery serves back, as a
    // TENANT card — the tier is what a client renders the chip from, and an
    // onboarding filed under the platform tier would leak into every gallery.
    let libraries = Libraries::new(database.clone());
    let mine = libraries
        .gallery(&parse(&workspace), WHOLE_PAGE, None)
        .await
        .expect("the gallery reads");
    let card = mine
        .items
        .iter()
        .find(|card| card.name == name)
        .expect("the onboarded bundle is in its own workspace's gallery");
    assert_eq!(card.tier, Tier::Tenant);

    // And the isolation half, which no ownership layer can answer for: the
    // statement itself filters on the workspace, so a neighbour's gallery does
    // not carry this row even though both read the same table.
    let theirs = libraries
        .gallery(&parse(&neighbour), WHOLE_PAGE, None)
        .await
        .expect("the neighbour's gallery reads");
    assert!(
        !theirs.items.iter().any(|card| card.name == name),
        "a workspace library is not visible from another workspace"
    );

    drop(database);
    lane.cleanup().await;
}

/// A tenant and one workspace under it, which the library rows reference.
///
/// `tenant_fleet_library.workspace_id` is a real foreign key into
/// `core.workspaces` — a minted id alone is refused, and rightly: a library
/// entry belonging to no workspace is exactly the orphan the constraint is
/// there to prevent.
async fn seed_scope(database: &afd_db::Db, workspace: &str) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "WITH tenant AS ( \
           INSERT INTO core.tenants (id, name, created_at, updated_at) \
           VALUES ($1::uuid, 'Gallery fixture', 1, 1) \
           RETURNING id \
         ) \
         INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
         SELECT $2::uuid, id, $2, 'test', 1 FROM tenant",
    )
    .bind(mint_id())
    .bind(workspace)
    .execute(&mut *connection)
    .await
    .expect("the gallery scope seeds");
}

/// A published platform row, visible to every workspace.
async fn seed_platform(database: &afd_db::Db, id: &str, created_at: i64) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.fleet_library ( \
           id, name, description, source_repo, source_path, source_ref, \
           required_credentials, required_credentials_reasons, required_tools, \
           network_hosts, visibility, content_hash, skill_markdown, trigger_markdown, \
           support_files_json, created_at, updated_at) \
         VALUES ($1, $1, 'platform fixture', $1, '', 'main', \
           '[]', '{}', '[]', '[]', 'public', 'abc123', '# Fixture', NULL, '[]', $2, $2)",
    )
    .bind(id)
    .bind(created_at)
    .execute(&mut *connection)
    .await
    .expect("the platform row seeds");
}

/// A row in one workspace's own library.
async fn seed_workspace(database: &afd_db::Db, workspace: &str, id: &str, created_at: i64) {
    let mut connection = database.acquire().await.expect("an API connection");
    sqlx::query(
        "INSERT INTO core.tenant_fleet_library ( \
           id, workspace_id, name, description, source_kind, source_ref, visibility, \
           content_hash, skill_markdown, trigger_markdown, support_files_json, \
           requirements_json, created_at, updated_at) \
         VALUES ($1::uuid, $2::uuid, $3, 'tenant fixture', 'github', 'main', 'workspace', \
           $5, '# Fixture', NULL, '[]', \
           '{\"credentials\":[],\"tools\":[],\"network_hosts\":[],\"trigger_present\":false}', \
           $4, $4)",
    )
    .bind(mint_id())
    .bind(workspace)
    .bind(id)
    .bind(created_at)
    // The domain key is (workspace_id, content_hash), so two rows in one
    // workspace need two hashes — a shared literal would make the second seed
    // an upsert over the first and leave the merge with nothing to interleave.
    .bind(format!("hash-{id}"))
    .execute(&mut *connection)
    .await
    .expect("the workspace row seeds");
}

/// The fixture identifier as the store takes it.
fn parse(id: &str) -> afd_core::id::Uuid7 {
    afd_core::id::Uuid7::parse(id).expect("the minted fixture id is UUIDv7")
}

/// A one-file bundle, gzipped as GitHub serves a tarball.
fn archive(name: &str) -> Vec<u8> {
    let document = format!(
        "---\nname: {name}\ndescription: deterministic onboarding\nversion: 1.0.0\n---\nRun."
    );
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(document.len()).expect("the fixture length fits"));
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "wrapper/SKILL.md", document.as_bytes())
        .expect("the fixture tar encodes");
    let bytes = tar.into_inner().expect("the fixture tar finishes");
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(&bytes).expect("the fixture gzip writes");
    gzip.finish().expect("the fixture gzip finishes")
}

/// A single-shot HTTP origin serving `body`, for the fetch to reach locally.
fn serve(body: &[u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the fixture server binds");
    let address = listener.local_addr().expect("the fixture address resolves");
    let body = body.to_vec();
    std::thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("the source request arrives");
        let mut request = [0_u8; 4096];
        let _read = socket.read(&mut request).expect("the request is readable");
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket
            .write_all(head.as_bytes())
            .expect("the response head writes");
        socket.write_all(&body).expect("the response body writes");
    });
    format!("http://{address}")
}
