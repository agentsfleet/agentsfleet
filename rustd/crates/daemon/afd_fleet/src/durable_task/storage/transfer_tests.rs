//! Transfer material reaches the runner's supervisor, never a log line.

use super::Authorization;
use std::collections::BTreeMap;

// Value: protects=a signed transfer URL and its headers never reach a log
// through Debug; fails_when=Debug prints the URL or a header; why_new=the
// redaction had no case; seam=the type's own Debug.
#[test]
fn an_authorization_debugs_as_redacted() {
    let authorization = Authorization {
        url: "https://bucket.example/object?X-Amz-Signature=secret".to_owned(),
        headers: BTreeMap::from([("x-amz-checksum-sha256".to_owned(), "digest".to_owned())]),
    };

    let printed = format!("{authorization:?}");

    assert_eq!(printed, "Authorization(redacted)");
}
