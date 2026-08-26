#![expect(clippy::expect_used, reason = "tests inspect crafted archives")]

use std::io::Write as _;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::{Repository, extract};
use crate::{Error, SourceFailure};

const LENGTH_FITS: &str = "fixture length fits";
const TAR_FINISHES: &str = "fixture tar finishes";
const GZIP_WRITES: &str = "fixture gzip writes";
const GZIP_FINISHES: &str = "fixture gzip finishes";

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(content.len()).expect(LENGTH_FITS));
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, path, *content)
            .expect("fixture tar encodes");
    }
    let bytes = tar.into_inner().expect(TAR_FINISHES);
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&bytes).expect(GZIP_WRITES);
    gzip.finish().expect(GZIP_FINISHES)
}

fn traversal_archive() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (path, content, traversal) in [
        ("wrapper/SKILL.md", b"skill".as_slice(), false),
        ("wrapper/xx/secret", b"value".as_slice(), true),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_path(path).expect("safe placeholder path");
        header.set_size(u64::try_from(content.len()).expect(LENGTH_FITS));
        header.set_mode(0o644);
        if traversal {
            header
                .as_mut_bytes()
                .get_mut(8..10)
                .expect("placeholder segment exists")
                .copy_from_slice(b"..");
        }
        header.set_cksum();
        tar.append(&header, content).expect("crafted tar encodes");
    }
    let bytes = tar.into_inner().expect(TAR_FINISHES);
    let mut gzip = GzEncoder::new(Vec::new(), Compression::default());
    gzip.write_all(&bytes).expect(GZIP_WRITES);
    gzip.finish().expect(GZIP_FINISHES)
}

#[test]
fn repository_parser_owns_url_segment_safety() {
    let repository = Repository::parse("agentsfleet/reviewer").expect("valid repository");
    assert_eq!(repository.owner, "agentsfleet");
    assert_eq!(repository.name, "reviewer");
    for invalid in [
        "missing-slash",
        "owner/",
        "/repo",
        "a/b/c",
        "../etc",
        "owner/re:po",
    ] {
        assert!(matches!(
            Repository::parse(invalid),
            Err(Error::Source(SourceFailure::InvalidReference))
        ));
    }
}

#[test]
fn github_archive_strips_wrapper_and_rejects_traversal() {
    let valid = archive(&[
        ("wrapper/SKILL.md", b"skill"),
        ("wrapper/docs/note.md", b"note"),
    ]);
    let bundle = extract(&valid, "agentsfleet/reviewer", "main").expect("safe archive extracts");
    assert_eq!(bundle.skill_markdown, b"skill");
    assert_eq!(
        bundle.support_files.first().expect("one support file").path,
        "docs/note.md"
    );

    let hostile = traversal_archive();
    assert!(matches!(
        extract(&hostile, "agentsfleet/reviewer", "main"),
        Err(Error::Source(SourceFailure::UnsafeArchive))
    ));
}

#[test]
fn github_archive_classifies_truncation() {
    let mut bytes = archive(&[("wrapper/SKILL.md", b"skill")]);
    bytes.truncate(bytes.len() / 2);
    assert!(matches!(
        extract(&bytes, "agentsfleet/reviewer", "main"),
        Err(Error::Archive(_) | Error::Source(SourceFailure::Truncated))
    ));
}
