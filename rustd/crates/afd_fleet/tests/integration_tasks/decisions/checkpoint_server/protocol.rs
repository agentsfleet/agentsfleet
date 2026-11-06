//! Bounded fixture HTTP parsing; assertions reject unexpected request shapes.

use std::collections::BTreeMap;
use tokio::io::AsyncReadExt as _;
use tokio_rustls::server::TlsStream;

pub(super) struct Request {
    pub method: String,
    pub path: String,
    pub bulk_delete: bool,
    pub create_only: bool,
    pub body: Vec<u8>,
}

pub(super) async fn read(stream: &mut TlsStream<tokio::net::TcpStream>) -> Request {
    let mut bytes = Vec::new();
    let (end, length, method, path, create_only) = loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.expect("request bytes");
        assert!(count > 0, "complete request headers");
        bytes.extend_from_slice(chunk.get(..count).expect("read fits buffer"));
        assert!(bytes.len() < 32 * 1024, "fixture request bounded");
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let (length, method, path, create_only) =
                headers(bytes.get(..end).expect("header boundary"));
            break (end + 4, length, method, path, create_only);
        }
    };
    while bytes.len() - end < length {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.expect("body bytes");
        assert!(count > 0, "complete request body");
        bytes.extend_from_slice(chunk.get(..count).expect("read fits buffer"));
    }
    let (path, query) = path.split_once('?').unwrap_or((&path, ""));
    Request {
        method,
        path: path.to_owned(),
        bulk_delete: query == "delete",
        create_only,
        body: bytes
            .get(end..end + length)
            .expect("complete bounded body")
            .to_vec(),
    }
}

fn headers(bytes: &[u8]) -> (usize, String, String, bool) {
    let header = std::str::from_utf8(bytes).expect("HTTP header");
    let mut lines = header.lines();
    let first = lines
        .next()
        .expect("request line")
        .split_whitespace()
        .collect::<Vec<_>>();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect::<BTreeMap<_, _>>();
    let length = headers
        .get("content-length")
        .map_or(0, |value| value.parse::<usize>().expect("body size"));
    assert!(length < 8192, "small real transport fixture");
    (
        length,
        first.first().expect("request method").to_string(),
        first.get(1).expect("request path").to_string(),
        headers
            .get("if-none-match")
            .is_some_and(|value| value == "*"),
    )
}
