#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "tests inspect failures directly"
)]

use super::prepare;
use crate::{ImportBody, InvalidBundle, SourceKind, SupportFile};

const SKILL: &[u8] = b"---\nname: github-pr-reviewer\ndescription: Reviews pull requests\nversion: 0.1.0\n---\nBody.\n";
const KIBIBYTE: usize = 1024;

fn body() -> ImportBody {
    ImportBody {
        source_kind: SourceKind::Upload,
        source_ref: "unit".into(),
        source_revision: None,
        skill_markdown: SKILL.to_vec(),
        trigger_markdown: None,
        support_files: Vec::new(),
    }
}

fn invalid(body: &ImportBody) -> InvalidBundle {
    let error = prepare(body).expect_err("the hostile bundle must be refused");
    assert_eq!(error.code().as_str(), "UZ-BUNDLE-001");
    match error {
        crate::Error::Invalid(reason) => reason,
        crate::Error::Storage(_)
        | crate::Error::Catalogue(_)
        | crate::Error::Snapshot(_)
        | crate::Error::FrontmatterUtf8 { .. }
        | crate::Error::FrontmatterYaml { .. }
        | crate::Error::Source(_)
        | crate::Error::Github(_)
        | crate::Error::Archive(_) => {
            panic!("validation cannot reach an I/O boundary")
        }
    }
}

#[test]
fn test_bundle_import_rejects_hostile() {
    let cases = [
        (
            "../secret.txt",
            b"safe".as_slice(),
            InvalidBundle::UnsafeSupportPath,
        ),
        (
            "docs//note.md",
            b"safe".as_slice(),
            InvalidBundle::UnsafeSupportPath,
        ),
        (
            "key.txt",
            b"api_key: stolen".as_slice(),
            InvalidBundle::EmbeddedCredential,
        ),
    ];
    for (path, content, expected) in cases {
        let mut input = body();
        input.support_files.push(SupportFile {
            path: path.into(),
            content: content.to_vec(),
        });
        assert_eq!(invalid(&input), expected);
    }

    let mut oversized = body();
    oversized.support_files.push(SupportFile {
        path: "large.bin".into(),
        content: vec![0; 64 * KIBIBYTE + 1],
    });
    assert_eq!(invalid(&oversized), InvalidBundle::SupportFileTooLarge);

    let mut malformed = body();
    malformed.skill_markdown = b"not frontmatter".to_vec();
    assert_eq!(invalid(&malformed), InvalidBundle::InvalidSkill);
}

#[test]
fn manifest_contains_hashes_not_support_bytes() {
    let mut input = body();
    input.support_files.push(SupportFile {
        path: "README.md".into(),
        content: b"review notes".to_vec(),
    });
    let prepared = prepare(&input).expect("the bundle is valid");
    let encoded =
        serde_json::to_string(&prepared.support_manifest).expect("the manifest serializes");
    assert!(encoded.contains("README.md"));
    assert!(encoded.contains("sha256"));
    assert!(!encoded.contains("review notes"));
}
