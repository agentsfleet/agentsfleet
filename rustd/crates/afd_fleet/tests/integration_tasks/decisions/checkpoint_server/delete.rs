//! S3 multi-object deletion acknowledges precisely the requested private keys.

use super::protocol::Request;
use std::collections::BTreeMap;

const OPEN: &str = "<Delete xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">";

pub(super) fn objects(request: &Request, objects: &mut BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    assert_eq!(
        request.path, "/checkpoint-test",
        "delete names the configured bucket"
    );
    assert!(
        request.bulk_delete,
        "POST must explicitly request the S3 delete operation"
    );
    let body = std::str::from_utf8(&request.body).expect("S3 XML request");
    let mut remaining = body
        .strip_prefix(OPEN)
        .and_then(|body| body.strip_suffix("</Delete>"))
        .expect("complete S3 Delete document");
    let mut response =
        String::from("<DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");
    let mut count = 0;
    while !remaining.is_empty() {
        let item = remaining
            .strip_prefix("<Object><Key>")
            .expect("delete Object with one Key");
        let (key, rest) = item
            .split_once("</Key></Object>")
            .expect("closed object key");
        assert!(
            key.starts_with("capability-probes/") || key.starts_with("workspaces/"),
            "only native checkpoint keys may be deleted"
        );
        assert!(
            key.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'/' || byte == b'-'),
            "fixture native keys have no XML escapes or arbitrary path syntax"
        );
        objects.remove(&format!("/checkpoint-test/{key}"));
        response.push_str(&format!("<Deleted><Key>{key}</Key></Deleted>"));
        remaining = rest;
        count += 1;
    }
    assert!(
        (1..=1000).contains(&count),
        "S3 bounded nonempty delete batch"
    );
    response.push_str("</DeleteResult>");
    response.into_bytes()
}
