//! Build-surface and production-closure guards for provenance.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::metadata::dependency_closure;
use super::{Surface, selected};

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
