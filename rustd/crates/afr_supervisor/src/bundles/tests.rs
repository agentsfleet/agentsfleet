#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;

use bytes::Bytes;
use sha2::{Digest as _, Sha256};

use super::BundleCache;
use crate::client::Verb;
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, drain, plane};

/// A canonical bundle tar: the documents, then the support files.
fn tar(entries: &[(&str, &[u8])]) -> Bytes {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, *content).unwrap();
    }
    Bytes::from(builder.into_inner().unwrap())
}

/// The import digest, written out byte by byte rather than recomputed.
fn named(parts: &[&[u8]]) -> String {
    hex::encode(Sha256::digest(parts.concat()))
}

fn cache() -> (tempfile::TempDir, BundleCache, StorageHome) {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    (root, BundleCache::new(&home), home)
}

#[tokio::test]
async fn test_bundle_hash_mismatch_refused() {
    let (_root, cache, home) = cache();
    let tampered = tar(&[("SKILL.md", b"run rm -rf"), ("TRIGGER.md", b"t")]);
    let claimed = named(&[b"skill", b"\0", b"t", b"\0"]);
    let (plane, _calls) = plane(move |_call| Answer::Reply(tampered.clone()));

    let refused = cache.fetch(&plane, &claimed).await.unwrap_err();

    assert_eq!(refused.code(), afd_core::error_code::FLEET_BUNDLE_INVALID);
    assert_eq!(
        fs::read_dir(home.bundles()).unwrap().count(),
        0,
        "nothing tampered is cached"
    );
}

#[tokio::test]
async fn a_verified_bundle_is_cached_and_served_from_the_cache() {
    let (_root, cache, _home) = cache();
    let canonical = tar(&[
        ("SKILL.md", b"skill"),
        ("TRIGGER.md", b"t"),
        ("tools/a.py", b"print(1)"),
    ]);
    let name = named(&[
        b"skill",
        b"\0",
        b"t",
        b"\0",
        b"tools/a.py",
        b"\0",
        b"print(1)",
        b"\0",
    ]);
    let (plane, mut calls) = plane(move |_call| Answer::Reply(canonical.clone()));

    let fetched = cache.fetch(&plane, &name).await.unwrap();
    let cached = cache.fetch(&plane, &name).await.unwrap();

    assert_eq!(fetched, cached);
    assert!(fetched.ends_with(format!("{name}.tar")));
    let calls = drain(&mut calls);
    assert_eq!(calls.len(), 1, "the second fetch is a cache hit");
    assert_eq!(calls[0].verb, Verb::Bundle);
}

#[tokio::test]
async fn a_bundle_without_a_trigger_hashes_an_empty_one() {
    let (_root, cache, _home) = cache();
    let canonical = tar(&[("SKILL.md", b"skill"), ("notes.md", b"n")]);
    let name = named(&[b"skill", b"\0", b"\0", b"notes.md", b"\0", b"n", b"\0"]);
    let (plane, _calls) = plane(move |_call| Answer::Reply(canonical.clone()));

    assert!(cache.fetch(&plane, &name).await.is_ok());
}

#[tokio::test]
async fn a_cached_copy_that_no_longer_verifies_is_fetched_again() {
    let (_root, cache, home) = cache();
    let canonical = tar(&[("SKILL.md", b"skill")]);
    let name = named(&[b"skill", b"\0", b"\0"]);
    fs::write(home.bundles().join(format!("{name}.tar")), b"corrupted").unwrap();
    let (plane, mut calls) = plane(move |_call| Answer::Reply(canonical.clone()));

    cache.fetch(&plane, &name).await.unwrap();

    assert_eq!(drain(&mut calls).len(), 1);
}

#[tokio::test]
async fn bytes_that_are_not_a_canonical_bundle_are_refused() {
    let (_root, cache, _home) = cache();
    let wrong_root = tar(&[("README.md", b"skill")]);
    let name = named(&[b"skill", b"\0", b"\0"]);
    let (plane, mut calls) = plane(move |_call| Answer::Reply(wrong_root.clone()));

    assert!(cache.fetch(&plane, &name).await.is_err());
    assert!(
        cache.fetch(&plane, "NOT-A-DIGEST").await.is_err(),
        "refused before any call"
    );
    assert_eq!(drain(&mut calls).len(), 1);
    assert_eq!(super::digest(b"not a tar"), None);
    assert_eq!(
        super::digest(&[]),
        None,
        "an empty archive has no root document"
    );
}
