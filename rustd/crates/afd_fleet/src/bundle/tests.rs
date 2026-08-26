//! What the key builder admits, and what the store answers.
//!
//! Runs against `object_store::memory::InMemory` — the backend the workspace
//! manifest names for exactly this. No network, no credentials, and the code
//! under test is the same client production drives, which is what a hand-rolled
//! mock could not have given.
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;

use object_store::ObjectStoreExt as _;
use object_store::memory::InMemory;

use super::{Bundles, ContentHash, MAX_SNAPSHOT_BYTES};

/// A canonical digest: the SHA-256 of the empty input, which is 64 lowercase
/// hex characters and is not a value anyone has to invent.
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// A store holding one snapshot under `EMPTY_SHA256`.
async fn store_holding(body: &[u8]) -> Bundles {
    let store = InMemory::new();
    let hash = ContentHash::parse(EMPTY_SHA256).expect("the fixture digest is canonical");
    store
        .put(
            &hash.snapshot_key(),
            bytes::Bytes::copy_from_slice(body).into(),
        )
        .await
        .expect("an in-memory put cannot fail");
    Bundles::new(Arc::new(store))
}

#[test]
fn test_content_hash_admits_only_a_lowercase_digest() {
    ContentHash::parse(EMPTY_SHA256).expect("a canonical digest is a content hash");

    // Every rejection `bundles.zig`'s own unit test names, in its order.
    for refused in [
        "",
        &EMPTY_SHA256[..63],
        &format!("{EMPTY_SHA256}a"),
        &EMPTY_SHA256.to_uppercase(),
        "../../etc/passwd000000000000000000000000000000000000000000000000",
    ] {
        assert!(
            ContentHash::parse(refused).is_err(),
            "{refused:?} must not parse as a content hash"
        );
    }
}

#[test]
fn test_snapshot_key_matches_the_zig_layout() {
    let hash = ContentHash::parse(EMPTY_SHA256).expect("the fixture digest is canonical");
    // `importer.zig`'s SNAPSHOT_KEY_PREFIX ++ hash ++ SNAPSHOT_KEY_SUFFIX,
    // written out rather than assembled from the constants above: a test that
    // rebuilds the key the same way the code does would pass through any change
    // to either, and the whole point of this one is that the two
    // implementations address the same bucket.
    assert_eq!(
        hash.snapshot_key().as_ref(),
        "fleet-bundles/sha256/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.tar"
    );
}

#[tokio::test]
async fn test_fetch_returns_the_exact_stored_bytes() {
    // Deliberately not valid tar and deliberately not UTF-8: this daemon is a
    // proxy, and a byte it interprets is a byte it could get wrong.
    let body: &[u8] = &[0x00, 0xff, b'a', 0x7f, 0x80];
    let bundles = store_holding(body).await;
    let hash = ContentHash::parse(EMPTY_SHA256).expect("the fixture digest is canonical");

    let fetched = bundles.fetch(hash).await.expect("the snapshot is stored");

    assert_eq!(fetched.as_ref(), body);
}

#[tokio::test]
async fn test_fetch_of_an_unstored_hash_is_not_found() {
    let bundles = store_holding(b"stored").await;
    let absent =
        ContentHash::parse("0000000000000000000000000000000000000000000000000000000000000000")
            .expect("the absent digest is well formed");

    let error = bundles
        .fetch(absent)
        .await
        .expect_err("nothing is stored under that hash");

    assert_eq!(error.code().as_str(), "UZ-BUNDLE-002");
    assert_eq!(error.detail(), "no snapshot stored for this content hash");
}

#[tokio::test]
async fn test_an_unconfigured_store_is_unavailable_not_missing() {
    let hash = ContentHash::parse(EMPTY_SHA256).expect("the fixture digest is canonical");

    let error = Bundles::unconfigured()
        .fetch(hash)
        .await
        .expect_err("a deployment with no store cannot serve one");

    // The distinction this asserts is the one an operator acts on: a 404 would
    // send them looking for a bundle that was never the problem.
    assert_eq!(error.code().as_str(), "UZ-BUNDLE-005");
    assert_eq!(
        error.detail(),
        "Fleet Bundle snapshot storage is unavailable"
    );
}

#[tokio::test]
async fn test_an_oversized_snapshot_is_refused_rather_than_buffered() {
    let oversized = vec![0u8; usize::try_from(MAX_SNAPSHOT_BYTES).expect("the ceiling fits") + 1];
    let bundles = store_holding(&oversized).await;
    let hash = ContentHash::parse(EMPTY_SHA256).expect("the fixture digest is canonical");

    let error = bundles
        .fetch(hash)
        .await
        .expect_err("an object past the ceiling is not served");

    assert_eq!(error.code().as_str(), "UZ-BUNDLE-005");
    // The size stays out of the wire sentence and lives in the Display, which
    // is what the operator's log line carries.
    assert_eq!(error.detail(), "Fleet Bundle snapshot fetch failed");
    assert!(
        error
            .to_string()
            .contains(&(MAX_SNAPSHOT_BYTES + 1).to_string()),
        "the operator's line names the size that was refused: {error}"
    );
}
