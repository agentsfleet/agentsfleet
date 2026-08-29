use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::thread;

use super::super::{GithubSource, Repository};
use super::archive;
use crate::{BundleSource as _, Error, Result, SourceFailure};

fn redirecting_server(body: Vec<u8>, redirect_again: bool) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("fixture server binds");
    let address = listener.local_addr().expect("fixture address resolves");
    thread::spawn(move || {
        for request_index in 0..2 {
            let (mut socket, _peer) = listener.accept().expect("fixture request arrives");
            let mut request = [0_u8; 4096];
            let _read = socket.read(&mut request).expect("fixture request reads");
            if request_index == 0 || redirect_again {
                let head = format!(
                    "HTTP/1.1 302 Found\r\nLocation: http://{address}/archive\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                socket
                    .write_all(head.as_bytes())
                    .expect("fixture redirect writes");
            } else {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket
                    .write_all(head.as_bytes())
                    .expect("fixture response head writes");
                socket.write_all(&body).expect("fixture body writes");
            }
        }
    });
    format!("http://{address}")
}

fn allow_fixture_redirect(location: &str) -> Result<()> {
    if location.starts_with("http://127.0.0.1:") {
        Ok(())
    } else {
        Err(SourceFailure::DisallowedRedirect.into())
    }
}

#[tokio::test]
async fn redirect_protocol_accepts_one_validated_hop_only() {
    let repository = Repository::parse("agentsfleet/reviewer").expect("repository is valid");
    let body = archive(&[("wrapper/SKILL.md", b"redirected")]);
    let source = GithubSource::new("main")
        .expect("revision is valid")
        .pointed_at(redirecting_server(body, false));
    let compressed = source
        .download_with(&repository, allow_fixture_redirect)
        .await
        .expect("one approved redirect downloads");
    assert!(!compressed.is_empty());

    let source = GithubSource::new("main")
        .expect("revision is valid")
        .pointed_at(redirecting_server(Vec::new(), true));
    assert!(matches!(
        source
            .download_with(&repository, allow_fixture_redirect)
            .await,
        Err(Error::Source(SourceFailure::DisallowedRedirect))
    ));
}

#[tokio::test]
async fn redirect_protocol_rejects_missing_or_unapproved_locations() {
    for (status, expected) in [
        ("302 Found", SourceFailure::DisallowedRedirect),
        ("301 Moved Permanently", SourceFailure::DisallowedRedirect),
    ] {
        let source = GithubSource::new("main")
            .expect("revision is valid")
            .pointed_at(super::serve(status, Vec::new()));
        assert!(matches!(
            source.fetch("agentsfleet/reviewer").await,
            Err(Error::Source(actual)) if actual == expected
        ));
    }
}

#[tokio::test]
async fn compressed_downloads_stop_at_the_resource_ceiling() {
    let body = vec![0_u8; super::super::MAX_COMPRESSED_BYTES + 1];
    let source = GithubSource::new("main")
        .expect("revision is valid")
        .pointed_at(super::serve("200 OK", body));
    assert!(matches!(
        source.fetch("agentsfleet/reviewer").await,
        Err(Error::Source(SourceFailure::ArchiveTooLarge))
    ));
}
