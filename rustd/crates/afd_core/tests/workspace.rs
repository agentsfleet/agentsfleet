//! Workspace-wide guarantees: the toolchain pin, the lint policy, and the
//! dependency freeze that keeps `afd_core` and `afd_wire` free of a runtime.
//!
//! These assert properties of the WORKSPACE rather than of `afd_core` alone.
//! They live here because `afd_core` is the base crate every other member
//! depends on, so a failure surfaces before anything built on top of it runs.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test target: a malformed manifest or cargo-metadata document is an \
              unmet precondition, and failing loudly on it is the correct outcome"
)]

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Crates that would make `afd_core` or `afd_wire` something other than a pure
/// value layer: an asynchronous runtime, a datastore client, or a transport.
///
/// Matched on the package name, so a rename to a fork is caught by the missing
/// entry rather than slipping through a substring test.
const FORBIDDEN: &[&str] = &[
    "tokio",
    "async-std",
    "smol",
    "sqlx",
    "tokio-postgres",
    "postgres",
    "deadpool",
    "redis",
    "fred",
    "axum",
    "hyper",
    "tower",
    "reqwest",
    "actix-web",
];

fn workspace_root() -> PathBuf {
    // <root>/crates/afd_core -> <root>
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Extracts a `key = "value"` string from a manifest, ignoring comment lines so
/// a commented-out example cannot satisfy the assertion.
fn manifest_value(manifest: &str, key: &str) -> Option<String> {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?;
            Some(rest.trim().trim_matches('"').to_owned())
        })
}

/// Catches a pin that drifted from the compiler a developer actually runs —
/// the state where a lane passes locally and fails in Continuous Integration
/// because the two resolve different rustc versions.
#[test]
fn test_workspace_builds_pinned() {
    let root = workspace_root();
    let channel = manifest_value(&read(&root.join("rust-toolchain.toml")), "channel")
        .expect("rust-toolchain.toml must pin a channel");
    let floor = manifest_value(&read(&root.join("Cargo.toml")), "rust-version")
        .expect("workspace must declare rust-version");

    // Run inside the workspace so the rustup shim honours rust-toolchain.toml;
    // this is the compiler that built the binary executing this assertion.
    let out = Command::new("rustc")
        .arg("--version")
        .current_dir(&root)
        .output()
        .expect("rustc must be on PATH");
    assert!(out.status.success(), "rustc --version failed");
    let version = String::from_utf8(out.stdout).unwrap();

    assert!(
        version.contains(&channel),
        "resolved compiler {version:?} does not match rust-toolchain.toml channel {channel:?}"
    );
    assert!(
        channel.starts_with(&floor),
        "pinned channel {channel:?} does not satisfy the declared rust-version floor {floor:?}"
    );
}

/// Catches the policy failure that matters more than the policy itself: a crate
/// added to the workspace WITHOUT `[lints] workspace = true`, which silently
/// exempts it from every deny the root declares.
#[test]
fn test_workspace_lint_policy() {
    let root = workspace_root();
    let manifest = read(&root.join("Cargo.toml"));

    for lint in ["unwrap_used", "expect_used", "panic", "indexing_slicing"] {
        let declared = format!("{lint} = \"deny\"");
        assert!(
            manifest.contains(&declared),
            "workspace must declare `{declared}` so a library crate cannot abort on bad input"
        );
    }
    assert!(
        manifest.contains(r#"warnings = { level = "deny", priority = -1 }"#),
        "workspace must treat every rustc warning as an error"
    );

    let members = members(&manifest);
    assert!(!members.is_empty(), "workspace declares no members");
    for member in &members {
        let member_manifest = read(&root.join(member).join("Cargo.toml"));
        assert!(
            member_manifest.contains("[lints]") && member_manifest.contains("workspace = true"),
            "member {member} does not inherit the workspace lint policy \
             (add `[lints]` + `workspace = true` to its Cargo.toml)"
        );
    }
}

/// The `members = [...]` list, as declared.
fn members(manifest: &str) -> Vec<String> {
    let start = manifest
        .find("members = [")
        .expect("workspace must list members");
    let rest = &manifest[start..];
    let end = rest.find(']').expect("members list must be closed");
    rest[..end]
        .split('"')
        .filter(|part| part.contains('/'))
        .map(str::to_owned)
        .collect()
}

/// Catches Invariant 2 breaking by accident: a transitive dependency dragging an
/// asynchronous runtime, a datastore client, or a transport into a crate that
/// must stay a pure value layer. Walks the RESOLVED graph rather than the
/// declared dependency list, so an indirect pull is caught too.
#[test]
fn test_core_dependency_freeze() {
    let root = workspace_root();
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1"])
        .current_dir(&root)
        .output()
        .expect("cargo metadata must run");
    assert!(out.status.success(), "cargo metadata failed");

    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let nodes = metadata["resolve"]["nodes"].as_array().unwrap();

    // Package identifiers spell themselves differently for a path member
    // (`path+file:///...#0.26.2`) and a registry crate
    // (`registry+...#serde@1.0.229`), so the name comes from the `packages`
    // array rather than from parsing the identifier.
    let names: BTreeMap<&str, &str> = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pkg| {
            (
                pkg["id"].as_str().unwrap_or_default(),
                pkg["name"].as_str().unwrap_or_default(),
            )
        })
        .collect();

    for crate_name in ["afd_core", "afd_wire"] {
        let reached = reachable(nodes, &names, crate_name);
        let forbidden: Vec<&str> = FORBIDDEN
            .iter()
            .copied()
            .filter(|name| reached.contains(*name))
            .collect();
        assert!(
            forbidden.is_empty(),
            "{crate_name} reaches {forbidden:?} through its normal dependency graph; \
             Invariant 2 keeps these crates out of the value layer"
        );
    }
}

/// Package names reachable from `crate_name` through NORMAL dependencies only.
///
/// Development and build dependencies are excluded on purpose: a test helper is
/// not linked into the shipped library, and Invariant 2 is a claim about what
/// the library carries into the daemon.
fn reachable(
    nodes: &[serde_json::Value],
    names: &BTreeMap<&str, &str>,
    crate_name: &str,
) -> BTreeSet<String> {
    let id_of = |node: &serde_json::Value| node["id"].as_str().unwrap_or_default().to_owned();
    let name_of = |id: &str| names.get(id).copied().unwrap_or_default().to_owned();

    let start = nodes
        .iter()
        .find(|node| name_of(&id_of(node)) == crate_name)
        .unwrap_or_else(|| panic!("{crate_name} is not in the resolve graph"));

    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([id_of(start)]);
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.iter().find(|node| id_of(node) == id) else {
            continue;
        };
        for dep in node["deps"].as_array().into_iter().flatten() {
            let normal = dep["dep_kinds"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| kind["kind"].is_null());
            if !normal {
                continue;
            }
            let dep_id = dep["pkg"].as_str().unwrap_or_default().to_owned();
            if seen.insert(name_of(&dep_id)) {
                queue.push_back(dep_id);
            }
        }
    }
    seen
}
