#![expect(clippy::expect_used, reason = "tests inspect crafted archives")]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::thread;

use flate2::Compression;
use flate2::write::GzEncoder;

use super::{
    GithubSource, Repository, classify_status, extract, safe_relative, valid_revision,
    validate_redirect,
};
use crate::{BundleSource, Error, SourceFailure};

const LENGTH_FITS: &str = "fixture length fits";
const TAR_FINISHES: &str = "fixture tar finishes";
const GZIP_WRITES: &str = "fixture gzip writes";
const GZIP_FINISHES: &str = "fixture gzip finishes";

pub(super) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
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

fn serve(status: &str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server binds");
    let address = listener.local_addr().expect("fixture address resolves");
    let status = status.to_owned();
    thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("fixture request arrives");
        let mut request = [0_u8; 4096];
        let _read = socket.read(&mut request).expect("fixture request reads");
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket
            .write_all(head.as_bytes())
            .expect("fixture response head writes");
        socket.write_all(&body).expect("fixture body writes");
    });
    format!("http://{address}")
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

#[test]
fn github_status_classifier_distinguishes_not_found_and_rate_limit() {
    assert_eq!(classify_status(404), Some(SourceFailure::NotFound));
    assert_eq!(classify_status(403), Some(SourceFailure::RateLimited));
    assert_eq!(classify_status(429), Some(SourceFailure::RateLimited));
    assert_eq!(classify_status(200), None);
    assert_eq!(classify_status(500), None);
}

#[test]
fn revisions_and_redirects_accept_only_safe_github_segments() {
    for valid in ["main", "release-1.2.3", "abc_DEF"] {
        assert!(valid_revision(valid), "{valid}");
        assert!(GithubSource::new(valid).is_ok(), "{valid}");
    }
    for invalid in ["", ".", "..", "feature/work", "bad:ref", &"a".repeat(101)] {
        assert!(!valid_revision(invalid), "{invalid}");
        assert!(matches!(
            GithubSource::new(invalid),
            Err(Error::Source(SourceFailure::InvalidReference))
        ));
    }

    for allowed in [
        "https://api.github.com/repos/agentsfleet/reviewer/tarball/main",
        "https://codeload.github.com/agentsfleet/reviewer/tar.gz/main",
    ] {
        assert!(validate_redirect(allowed).is_ok(), "{allowed}");
    }
    for denied in [
        "http://codeload.github.com/archive",
        "https://example.com/archive",
        "not a url",
    ] {
        assert!(validate_redirect(denied).is_err(), "{denied}");
    }
}

#[test]
fn archive_requires_one_skill_and_preserves_optional_content() {
    let complete = archive(&[
        ("wrapper/SKILL.md", b"skill"),
        ("wrapper/TRIGGER.md", b"trigger"),
        ("wrapper/docs/note.md", b"note"),
        ("wrapper/.github/workflow.yml", b"ignored"),
    ]);
    let body = extract(&complete, "agentsfleet/reviewer", "main").expect("archive extracts");
    assert_eq!(
        body.trigger_markdown.as_deref(),
        Some(b"trigger".as_slice())
    );
    assert_eq!(body.support_files.len(), 1);

    for malformed in [
        archive(&[("wrapper/TRIGGER.md", b"trigger")]),
        archive(&[
            ("wrapper/SKILL.md", b"first"),
            ("wrapper/SKILL.md", b"second"),
        ]),
    ] {
        assert!(matches!(
            extract(&malformed, "agentsfleet/reviewer", "main"),
            Err(Error::Source(
                SourceFailure::Truncated | SourceFailure::UnsafeArchive
            ))
        ));
    }
}

#[test]
fn archive_and_relative_path_bounds_fail_closed() {
    assert!(matches!(
        extract(&[], "agentsfleet/reviewer", "main"),
        Err(Error::Source(SourceFailure::Truncated))
    ));
    assert!(matches!(
        extract(b"not gzip", "agentsfleet/reviewer", "main"),
        Err(Error::Archive(_))
    ));

    for unsafe_path in [
        b"/wrapper/SKILL.md".as_slice(),
        b"wrapper\\SKILL.md",
        b"wrapper\0SKILL.md",
        b"wrapper//SKILL.md",
        b"wrapper/../SKILL.md",
        b"wrapper",
        &[0xff, b'/', b'x'],
    ] {
        assert!(safe_relative(unsafe_path).is_err(), "{unsafe_path:?}");
    }
    assert_eq!(
        safe_relative(b"wrapper/.hidden/file").expect("the hidden path is readable"),
        None
    );
    assert_eq!(
        safe_relative(b"wrapper/docs/note.md").expect("the path is safe"),
        Some("docs/note.md".to_owned())
    );
}

#[tokio::test]
async fn github_fetch_downloads_and_extracts_a_direct_archive() {
    let compressed = archive(&[
        ("wrapper/SKILL.md", b"# fetched"),
        ("wrapper/docs/note.md", b"note"),
    ]);
    let source = GithubSource::new("main")
        .expect("the revision is safe")
        .pointed_at(serve("200 OK", compressed));

    let fetched = source
        .fetch("agentsfleet/reviewer")
        .await
        .expect("the fixture archive fetches");

    assert_eq!(fetched.skill_markdown, b"# fetched");
    assert_eq!(fetched.source_ref, "agentsfleet/reviewer");
    assert_eq!(fetched.source_revision.as_deref(), Some("main"));
    assert_eq!(fetched.support_files.len(), 1);
}

#[tokio::test]
async fn github_fetch_preserves_actionable_status_classes() {
    for (status, expected) in [
        ("404 Not Found", SourceFailure::NotFound),
        ("429 Too Many Requests", SourceFailure::RateLimited),
    ] {
        let source = GithubSource::new("main")
            .expect("the revision is safe")
            .pointed_at(serve(status, Vec::new()));
        assert!(matches!(
            source.fetch("agentsfleet/reviewer").await,
            Err(Error::Source(actual)) if actual == expected
        ));
    }

    let source = GithubSource::new("main")
        .expect("the revision is safe")
        .pointed_at(serve("500 Internal Server Error", Vec::new()));
    assert!(matches!(
        source.fetch("agentsfleet/reviewer").await,
        Err(Error::Github(_))
    ));
}

mod limits;
mod transport;
