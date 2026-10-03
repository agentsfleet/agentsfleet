#![expect(
    clippy::expect_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::io::Read as _;

use afd_core::bundle::BundleDigest;

use super::{SKILL_PATH, TRIGGER_PATH, canonical_snapshot};
use crate::{ImportBody, SourceKind, SupportFile, prepare};

/// The first-party `ci-responder` documents, from the corpus the runtime's
/// suites read, so the trigger is one the importer accepts.
const CI_RESPONDER_SKILL: &[u8] =
    include_bytes!("../../../../../tests/fixtures/fleetbundle/ci-responder/SKILL.md");
const CI_RESPONDER_TRIGGER: &[u8] =
    include_bytes!("../../../../../tests/fixtures/fleetbundle/ci-responder/TRIGGER.md");

/// That bundle with support files in more than one directory.
fn bundle() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: "unit".into(),
        source_revision: None,
        skill_markdown: CI_RESPONDER_SKILL.to_vec(),
        trigger_markdown: Some(CI_RESPONDER_TRIGGER.to_vec()),
        support_files: vec![
            SupportFile {
                path: "docs/guide.md".into(),
                content: b"guide".to_vec(),
            },
            SupportFile {
                path: "scripts/check.sh".into(),
                content: b"exit 0".to_vec(),
            },
        ],
    }
}

/// Reading the stored archive back the way the runner does — instructions,
/// then the trigger when present, then each support file in stored order —
/// names the bundle exactly as the importer did. A path the archive writer
/// normalizes would make this disagree, and the runner would refuse the fleet.
#[test]
fn the_runner_names_the_stored_archive_as_the_importer_did() {
    let body = bundle();
    let named = prepare(&body).expect("the bundle prepares").content_hash;
    let snapshot = canonical_snapshot(&body).expect("the bundle snapshots");

    let mut archive = tar::Archive::new(snapshot.as_ref());
    let mut entries: Vec<(String, Vec<u8>)> = archive
        .entries()
        .expect("the snapshot is an archive")
        .map(|entry| {
            let mut entry = entry.expect("every entry reads");
            let path = entry.path().expect("a path").to_string_lossy().into_owned();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).expect("its bytes read");
            (path, content)
        })
        .collect();
    let (skill_path, skill) = entries.remove(0);
    assert_eq!(skill_path, SKILL_PATH);
    let trigger = (entries.first().map(|(path, _)| path.as_str()) == Some(TRIGGER_PATH))
        .then(|| entries.remove(0).1);
    let mut digest = BundleDigest::new(&skill, trigger.as_deref());
    for (path, content) in &entries {
        digest.support_file(path, content);
    }

    assert_eq!(digest.finish(), named);
}
