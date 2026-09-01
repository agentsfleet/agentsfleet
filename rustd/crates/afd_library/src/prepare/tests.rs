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
    match error {
        crate::Error::Invalid(reason) => reason,
        crate::Error::Storage(_)
        | crate::Error::StorageUnavailable
        | crate::Error::CatalogIdCollision { .. }
        | crate::Error::Pool(_)
        | crate::Error::CatalogueJson(_)
        | crate::Error::Database { .. }
        | crate::Error::Snapshot(_)
        | crate::Error::FrontmatterUtf8 { .. }
        | crate::Error::FrontmatterYaml { .. }
        | crate::Error::TriggerConfig(_)
        | crate::Error::Source(_)
        | crate::Error::Github(_)
        | crate::Error::Archive(_)
        | crate::Error::ArchiveTask(_)
        | crate::Error::Redirect(_)
        | crate::Error::ArchivePath(_)
        | crate::Error::Entropy { .. }
        | crate::Error::Mint { .. } => {
            panic!("validation cannot reach an I/O boundary")
        }
    }
}

#[test]
fn trigger_uses_the_full_runtime_schema() {
    for runtime in [
        "tools: [http_request]\n  budget:\n    daily_dollars: 1",
        "triggers:\n    - type: cron\n      schedule: '0 * * * *'\n  tools: [http_request]\n  buget:\n    daily_dollars: 1",
    ] {
        let mut input = body();
        input.trigger_markdown = Some(
            format!("---\nname: github-pr-reviewer\nx-agentsfleet:\n  {runtime}\n---\n")
                .into_bytes(),
        );
        let error = prepare(&input).expect_err("an incomplete or misspelled runtime is refused");
        assert!(matches!(&error, crate::Error::TriggerConfig(_)));
        assert_eq!(error.code().as_str(), "UZ-BUNDLE-001");
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

    for document in ["SKILL.md", "TRIGGER.md"] {
        let mut input = body();
        let hostile = b"---\nname: github-pr-reviewer\ndescription: Reviews pull requests\nversion: 0.1.0\n---\napi_key: stolen\n".to_vec();
        match document {
            "SKILL.md" => input.skill_markdown = hostile,
            "TRIGGER.md" => input.trigger_markdown = Some(hostile),
            _ => panic!("the fixture names a root document"),
        }
        assert_eq!(invalid(&input), InvalidBundle::EmbeddedCredential);
    }

    let mut oversized = body();
    oversized.support_files.push(SupportFile {
        path: "large.bin".into(),
        content: vec![0; 64 * KIBIBYTE + 1],
    });
    let error = prepare(&oversized).expect_err("the oversized file is refused");
    assert_eq!(error.code().as_str(), "UZ-REQ-002");
    assert!(matches!(
        error,
        crate::Error::Invalid(InvalidBundle::SupportFileTooLarge)
    ));

    let mut malformed = body();
    malformed.skill_markdown = b"not frontmatter".to_vec();
    assert_eq!(invalid(&malformed), InvalidBundle::InvalidSkill);

    let mut malformed_fence = body();
    malformed_fence.skill_markdown = b"---\nname: github-pr-reviewer\ndescription: Reviews pull requests\nversion: 0.1.0\n---garbage\nBody.\n".to_vec();
    assert_eq!(invalid(&malformed_fence), InvalidBundle::InvalidSkill);
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
