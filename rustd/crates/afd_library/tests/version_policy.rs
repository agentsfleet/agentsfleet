//! Import and installation agree on the supported version vocabulary.
#![expect(clippy::expect_used, reason = "test prerequisites must fail loudly")]

use afd_fleet_runtime::Version;
use afd_library::{ImportBody, SourceKind, prepare};

fn bundle(version: &str) -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: "version-policy".into(), source_revision: None,
        skill_markdown: format!("---\nname: version-policy\ndescription: A version fixture\nversion: '{version}'\n---\nInstructions.\n").into_bytes(),
        trigger_markdown: None, support_files: Vec::new(),
    }
}

#[test]
fn stable_versions_are_importable_and_installable() {
    for version in ["0.0.0", "1.2.3", "18446744073709551615.0.0"] {
        prepare(&bundle(version)).expect("a stable bundle version");
        Version::parse(version).expect("a stable runtime version");
    }
}

#[test]
fn unsupported_suffixes_are_refused_before_import() {
    for version in ["1.0.0-alpha", "1.0.0+build", "1.0.0-alpha+build"] {
        prepare(&bundle(version)).expect_err("an unsupported bundle version");
        Version::parse(version).expect_err("an unsupported runtime version");
    }
}

#[test]
fn invalid_or_overflowing_components_are_refused_at_both_boundaries() {
    for version in ["01.0.0", "1.0", "1.0.0.0", "18446744073709551616.0.0"] {
        prepare(&bundle(version)).expect_err("an unsupported bundle version");
        Version::parse(version).expect_err("an unsupported runtime version");
    }
}
