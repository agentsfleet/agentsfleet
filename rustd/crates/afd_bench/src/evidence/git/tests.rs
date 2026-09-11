//! Build-surface and production-closure guards for provenance.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::metadata::dependency_closure;
use super::{Surface, capture_only, selected};

#[test]
fn production_build_provenance_covers_toolchain_and_build_scripts() {
    assert!(selected("rustd/rust-toolchain.toml", Surface::Build));
    assert!(selected("rustd/crates/afd_api/build.rs", Surface::Build));
    assert!(selected("docs/metrics.census.tsv", Surface::Build));
    assert!(!selected("rustd/crates/afd_bench/build.rs", Surface::Build));
}

#[test]
fn agentsfleetd_dependency_closure_excludes_the_benchmark_harness() {
    let closure = dependency_closure("HEAD").expect("production metadata must resolve offline");
    let rendered = String::from_utf8(closure).expect("canonical metadata is UTF-8 JSON");

    assert!(
        !rendered.contains("afd_bench@"),
        "the rig-only harness must not enter a production package dependency closure"
    );
}

#[test]
fn capture_allows_only_its_own_untracked_archive() {
    let campaign = "m192-example";
    let archive = b"?? bench/baselines/datastore/m192-example/provenance.json\0";
    assert!(capture_only(archive, campaign));

    let dirty_source = b" M rustd/crates/afd_bench/src/lib.rs\0";
    assert!(
        !capture_only(dirty_source, campaign),
        "tracked source bytes cannot be attributed to HEAD"
    );
    let foreign_archive = b"?? bench/baselines/datastore/another-run/result.json\0";
    assert!(
        !capture_only(foreign_archive, campaign),
        "another campaign is not part of this capture"
    );
}
