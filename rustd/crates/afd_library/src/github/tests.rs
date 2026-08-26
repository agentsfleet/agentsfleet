#![expect(clippy::expect_used, reason = "tests inspect crafted archives")]

use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::{Repository, extract};
use crate::{Error, SourceFailure};

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(content.len()).expect("fixture length fits"));
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, *content).expect("fixture tar encodes");
    }
    let bytes = tar.into_inner().expect("fixture tar finishes");
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&bytes).expect("fixture gzip writes");
    gzip.finish().expect("fixture gzip finishes")
}

#[test]
fn repository_parser_owns_url_segment_safety() {
    let repository = Repository::parse("agentsfleet/reviewer").expect("valid repository");
    assert_eq!(repository.owner, "agentsfleet");
    assert_eq!(repository.name, "reviewer");
    for invalid in ["missing-slash", "owner/", "/repo", "a/b/c", "../etc", "owner/re:po"] {
        assert!(matches!(Repository::parse(invalid), Err(Error::Source(SourceFailure::InvalidReference))));
    }
}

#[test]
fn github_archive_strips_wrapper_and_rejects_traversal() {
    let valid = archive(&[("wrapper/SKILL.md", b"skill"), ("wrapper/docs/note.md", b"note")]);
    let bundle = extract(&valid, "agentsfleet/reviewer", "main").expect("safe archive extracts");
    assert_eq!(bundle.skill_markdown, b"skill");
    assert_eq!(bundle.support_files[0].path, "docs/note.md");

    let hostile = archive(&[("wrapper/SKILL.md", b"skill"), ("wrapper/../../secret", b"value")]);
    assert!(matches!(extract(&hostile, "agentsfleet/reviewer", "main"), Err(Error::Source(SourceFailure::UnsafeArchive))));
}

#[test]
fn github_archive_classifies_truncation() {
    let mut bytes = archive(&[("wrapper/SKILL.md", b"skill")]);
    bytes.truncate(bytes.len() / 2);
    assert!(matches!(extract(&bytes, "agentsfleet/reviewer", "main"), Err(Error::Archive(_)) | Err(Error::Source(SourceFailure::Truncated))));
}
