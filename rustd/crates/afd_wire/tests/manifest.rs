//! What the shipped daemon resolves, asserted against the manifests themselves.
//!
//! # Why this is not gated on the feature it grades
//!
//! The claim is about the build WITHOUT `openapi`: that a daemon compiled the
//! ordinary way carries `serde` and `serde_json` and nothing else. A test that only
//! ran under the feature could not make that claim, so this one reads the
//! manifests and runs in both builds.
//!
//! # Why the manifest and not `cargo tree`
//!
//! `cargo tree` is the direct measurement and it is the wrong tool here: it
//! needs a resolver run and a network path, inside a test that must stay
//! hermetic. The manifest is where the fact is DECLARED — `optional = true`
//! plus a feature no default names is exactly what keeps utoipa out of the
//! graph — and `http_plane_dependency_graph.rs` grades the composition root the
//! same way for the same reason. The measurement is R3's job on the command
//! line; keeping the declaration honest is this one's.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test target: an unreadable manifest is a precondition failure, not a result"
)]

use std::fs;
use std::path::{Path, PathBuf};

/// The feature every crate spells to turn schema generation on.
const FEATURE: &str = "openapi";

/// The dependency that feature is the only way to acquire.
const GENERATOR: &str = "utoipa";

/// The attribute an override is spelled with, and the word its prose must name.
const OVERRIDE: &str = "value_type";

/// Every crate that declares the feature, and therefore must declare it alike.
const DECLARING_CRATES: [&str; 1] = ["afd_wire"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("afd_wire must remain under rustd/crates")
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

/// The generator is optional wherever it is named, and no default turns it on.
///
/// Both halves are load-bearing and neither implies the other: a
/// non-optional dependency is in the graph whatever the features say, and an
/// optional one named by `default` is in the graph anyway.
#[test]
fn the_default_build_carries_no_schema_generator() {
    for crate_name in DECLARING_CRATES {
        let manifest = manifest(crate_name);

        let declaration = manifest
            .lines()
            .find(|line| line.trim_start().starts_with(GENERATOR))
            .unwrap_or_else(|| {
                panic!("{crate_name} declares the {FEATURE} feature but never names {GENERATOR}")
            });
        assert!(
            declaration.contains("optional = true"),
            "{crate_name} must take {GENERATOR} as an optional dependency, or the \
             shipped daemon resolves it: {declaration}"
        );

        let defaults = manifest
            .lines()
            .find(|line| line.trim_start().starts_with("default = ["));
        assert!(
            !defaults.is_some_and(|line| line.contains(FEATURE)),
            "{crate_name} names {FEATURE} in its default features, which puts \
             {GENERATOR} back in every build"
        );
    }
}

/// Every `value_type` override states why it is there.
///
/// An override is the one place a schema can silently disagree with what the
/// daemon serializes — it REPLACES the derived shape with an asserted one — so
/// the rule is that each names the difference it exists for.
///
/// # Why the predicate is the word and not merely a comment
///
/// "Some doc comment sits above it" is what this asked first, and it was a test
/// that could not fail: `missing_docs` is denied workspace-wide, so EVERY field
/// already carries prose and the check passed on a stripped justification. What
/// distinguishes a justified override is prose about the OVERRIDE, so the
/// predicate is that the run above names `value_type` — which is what all six
/// of them do, and what a seventh added without thought would not.
///
/// `//` and not only `///`: what an override justifies is a Rust-versus-wire
/// mismatch, which is a note to whoever maintains the type and not something
/// the published contract should carry. Those runs were demoted to plain
/// comments when the generated document began publishing doc comments.
#[test]
fn every_value_type_override_names_its_serialized_difference() {
    let source = workspace_root().join("crates").join("afd_wire").join("src");
    let mut unjustified = Vec::new();

    for file in source_files(&source) {
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", file.display()));
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            if !line.contains(OVERRIDE) || !line.trim_start().starts_with("#[") {
                continue;
            }
            // The justification is the doc-comment run immediately above the
            // attribute, skipping any other attributes between the two. It has
            // to NAME the override, or a field's ordinary documentation would
            // satisfy a check the author never thought about.
            let justified = lines[..number]
                .iter()
                .rev()
                .skip_while(|above| above.trim_start().starts_with("#["))
                .take_while(|above| above.trim_start().starts_with("//"))
                .any(|above| above.contains(OVERRIDE));
            if !justified {
                unjustified.push(format!("{}:{}", file.display(), number + 1));
            }
        }
    }

    assert!(
        unjustified.is_empty(),
        "every `value_type` override must carry a comment naming how the \
         serialized form differs from the Rust form.\nUnjustified: {unjustified:#?}"
    );
}

/// Every `.rs` file under `root`, recursively.
fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let entries =
            fs::read_dir(&directory).unwrap_or_else(|error| panic!("cannot walk source: {error}"));
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push(path);
            }
        }
    }
    found
}
