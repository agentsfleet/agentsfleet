//! S3 multi-object deletion acknowledges precisely the requested private keys.

use super::protocol::Request;
use std::{collections::BTreeMap, fmt::Write as _};

const NAMESPACE: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

#[derive(serde::Deserialize)]
enum Document {
    Delete(Batch),
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    #[serde(rename = "@xmlns")]
    namespace: String,
    #[serde(rename = "Object")]
    objects: Vec<Object>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Object {
    #[serde(rename = "Key")]
    key: String,
}

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
    let Document::Delete(batch) =
        quick_xml::de::from_str(body).expect("structured S3 Delete document");
    assert_eq!(batch.namespace, NAMESPACE, "S3 Delete namespace");
    assert!(
        (1..=1000).contains(&batch.objects.len()),
        "S3 bounded nonempty delete batch"
    );
    let mut response =
        String::from("<DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">");
    for Object { key } in batch.objects {
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
        write!(response, "<Deleted><Key>{key}</Key></Deleted>")
            .expect("write XML response to String");
    }
    response.push_str("</DeleteResult>");
    response.into_bytes()
}
