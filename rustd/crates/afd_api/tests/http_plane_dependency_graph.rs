//! The API composition graph stays acyclic and sibling-shaped.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "structural test setup must stop at the first unreadable workspace manifest"
)]

use std::fs;
use std::path::{Path, PathBuf};

const HTTP_CRATE: &str = "afd_http";
const COMPOSITION_CRATE: &str = "afd_api";
const PLANES: [&str; 3] = ["afd_api_tenant", "afd_api_runner", "afd_api_operator"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("afd_api must remain under rustd/crates")
        .to_owned()
}

fn manifest(crate_name: &str) -> String {
    let path = workspace_root()
        .join("crates")
        .join(crate_name)
        .join("Cargo.toml");
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn dependency_declared(manifest: &str, crate_name: &str) -> bool {
    manifest.lines().any(|line| {
        line.trim_start()
            .starts_with(&format!("{crate_name}.workspace = true"))
    })
}

#[test]
fn http_plane_dependency_graph_is_acyclic_and_sibling_shaped() {
    let root = manifest(COMPOSITION_CRATE);
    for plane in PLANES {
        assert!(
            dependency_declared(&root, plane),
            "composition root must depend on {plane}"
        );
        let plane_manifest = manifest(plane);
        assert!(
            dependency_declared(&plane_manifest, HTTP_CRATE),
            "{plane} must depend on the shared HTTP substrate"
        );
        assert!(
            !dependency_declared(&plane_manifest, COMPOSITION_CRATE),
            "{plane} must not point back to the composition root"
        );
        for sibling in PLANES {
            if sibling != plane {
                assert!(
                    !dependency_declared(&plane_manifest, sibling),
                    "{plane} must not depend on sibling {sibling}"
                );
            }
        }
    }
    assert!(
        !PLANES
            .iter()
            .any(|plane| dependency_declared(&manifest(HTTP_CRATE), plane)),
        "the shared substrate must not point upward into a handler plane"
    );
}
