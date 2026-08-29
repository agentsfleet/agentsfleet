//! Public source-import verbs through a deterministic HTTP transport.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;

use afd_core::clock::UnixMillis;
use afd_db::config::DbRole;
use afd_db::test_util::{TestDatabase, mint_id};
use afd_library::{Libraries, LibraryImports};
use flate2::Compression;
use flate2::write::GzEncoder;

const NOW: UnixMillis = UnixMillis::from_millis(1_725_000_000_000);

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn github_and_template_imports_use_the_mockable_transport_boundary() {
    let lane = TestDatabase::shared();
    let database = lane.open(DbRole::Api, &[]).await;
    let github_name = format!("github-{}", mint_id().replace('-', ""));
    let github = LibraryImports::without_store(database.clone())
        .with_github_api_base(serve(&archive(&github_name)));
    let imported = github
        .github("agentsfleet/reviewer", Some("main"), false, NOW)
        .await
        .expect("the GitHub source imports through the local transport");
    assert_eq!(imported.name, github_name);

    let template_name = format!("template-{}", mint_id().replace('-', ""));
    let templates = LibraryImports::without_store(database.clone())
        .with_github_api_base(serve(&archive(&template_name)));
    let imported = templates
        .template("reviewer", false, NOW)
        .await
        .expect("the first-party template imports through the same seam");
    assert_eq!(imported.name, template_name);
    let row = Libraries::new(database.clone())
        .list()
        .await
        .expect("the catalogue lists")
        .into_iter()
        .find(|entry| entry.id() == template_name)
        .expect("the template row was persisted");
    assert_eq!(row.source_ref(), "reviewer");

    drop(database);
    lane.cleanup().await;
}

fn archive(name: &str) -> Vec<u8> {
    let document = format!(
        "---\nname: {name}\ndescription: deterministic source import\nversion: 1.0.0\n---\nRun."
    );
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(u64::try_from(document.len()).expect("the fixture length fits"));
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "wrapper/SKILL.md", document.as_bytes())
        .expect("the fixture tar encodes");
    let bytes = tar.into_inner().expect("the fixture tar finishes");
    let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
    gzip.write_all(&bytes).expect("the fixture gzip writes");
    gzip.finish().expect("the fixture gzip finishes")
}

fn serve(body: &[u8]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("the fixture server binds");
    let address = listener.local_addr().expect("the fixture address resolves");
    let body = body.to_vec();
    std::thread::spawn(move || {
        let (mut socket, _peer) = listener.accept().expect("the source request arrives");
        let mut request = [0_u8; 4096];
        let _read = socket.read(&mut request).expect("the request is readable");
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        socket
            .write_all(head.as_bytes())
            .expect("the response head writes");
        socket.write_all(&body).expect("the response body writes");
    });
    format!("http://{address}")
}
