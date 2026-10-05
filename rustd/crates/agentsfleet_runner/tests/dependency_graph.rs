//! What the runner binary links, read from the resolved dependency graph.
#![expect(
    clippy::expect_used,
    reason = "test target: an unreadable graph should fail the test loudly"
)]

use std::collections::{BTreeSet, VecDeque};

use cargo_metadata::{DependencyKind, MetadataCommand, Node, PackageId};

/// The daemon crates a runner may link: the wire, the value layer, the
/// bounds the wire's types declare (`afd_validate`, which depends on garde
/// alone), the telemetry vocabulary, and the OTLP transport both binaries
/// export through. Anything else from the daemon is the control plane.
const ALLOWED_DAEMON_CRATES: [&str; 5] = [
    "afd_wire",
    "afd_core",
    "afd_validate",
    "afd_observability",
    "afd_otlp",
];

/// The transport the runner exports through, which the walk must reach.
const TRANSPORT: &str = "afd_otlp";

/// The prefix every daemon crate carries.
const DAEMON_PREFIX: &str = "afd_";

/// Datastore clients a runner never holds; the daemon owns every datastore.
const DATASTORE_CRATES: [&str; 2] = ["sqlx", "redis"];

/// This crate, whose normal graph is walked.
const RUNNER: &str = env!("CARGO_PKG_NAME");

/// Every package the runner binary links in a normal build.
fn linked() -> BTreeSet<String> {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(manifest)
        .exec()
        .expect("cargo answers with the workspace's metadata");
    let resolve = metadata.resolve.expect("metadata carries a resolved graph");
    let node = |id: &PackageId| -> &Node {
        resolve
            .nodes
            .iter()
            .find(|node| &node.id == id)
            .expect("every resolved id has a node")
    };
    let root = metadata
        .packages
        .iter()
        .find(|package| package.name.as_str() == RUNNER)
        .expect("the runner is a workspace member");
    let mut seen = BTreeSet::new();
    let mut pending = VecDeque::from([root.id.clone()]);
    while let Some(id) = pending.pop_front() {
        for dependency in &node(&id).deps {
            let normal = dependency
                .dep_kinds
                .iter()
                .any(|kind| kind.kind == DependencyKind::Normal);
            if normal && seen.insert(dependency.pkg.clone()) {
                pending.push_back(dependency.pkg.clone());
            }
        }
    }
    metadata
        .packages
        .iter()
        .filter(|package| seen.contains(&package.id))
        .map(|package| package.name.to_string())
        .collect()
}

#[test]
fn test_runner_links_no_datastore_crate() {
    let linked = linked();
    // The walk itself is proven before its silence is trusted: a broken walk
    // that found nothing would otherwise pass every assertion below.
    assert!(
        linked.contains("afd_wire"),
        "the walk reached the wire: {linked:?}"
    );
    assert!(
        linked.contains(TRANSPORT),
        "the runner exports through the shared transport: {linked:?}"
    );

    let datastores: Vec<_> = linked
        .iter()
        .filter(|name| DATASTORE_CRATES.contains(&name.as_str()))
        .collect();
    assert!(datastores.is_empty(), "the runner links {datastores:?}");
    let control_plane: Vec<_> = linked
        .iter()
        .filter(|name| name.starts_with(DAEMON_PREFIX))
        .filter(|name| !ALLOWED_DAEMON_CRATES.contains(&name.as_str()))
        .collect();
    assert!(
        control_plane.is_empty(),
        "the runner links {control_plane:?}"
    );
}
