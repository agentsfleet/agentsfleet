#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::BTreeMap;

use bytes::Bytes;
use serde_json::value::RawValue;

use super::{DELIMITER, WriteParams, decoded, line};

/// Bytes that are not text: a zero, a byte past ASCII, then a word.
const BINARY: &[u8] = b"\x00\xffhi";

fn raw(line: &Bytes) -> &RawValue {
    let text = std::str::from_utf8(line.strip_suffix(&[DELIMITER]).unwrap()).unwrap();
    serde_json::from_str(text).unwrap()
}

#[test]
fn bytes_travel_as_base64_and_come_back_unchanged() {
    let sent = WriteParams {
        process_id: 7,
        data: Bytes::from_static(BINARY),
    };

    let written = line(&sent);
    let read: WriteParams = decoded(raw(&written)).unwrap();

    assert!(
        written
            .windows(8)
            .any(|window| window == b"AP9oaQ==".as_slice())
    );
    assert_eq!(read.data, BINARY);
    assert_eq!(read.process_id, 7);
}

#[test]
fn bytes_that_are_not_base64_do_not_decode() {
    let written = Bytes::from_static(b"{\"process_id\":1,\"data\":\"not base64!\"}\n");

    let refused = decoded::<WriteParams>(raw(&written)).unwrap_err();

    assert!(refused.is_data(), "{refused}");
}

#[test]
fn a_line_ends_in_one_delimiter_and_an_unwritable_message_is_empty() {
    // A map keyed by something other than a string has no JSON spelling.
    let unwritable = BTreeMap::from([((1, 2), 3)]);

    let written = line(&WriteParams {
        process_id: 1,
        data: Bytes::new(),
    });

    let body = written.strip_suffix(&[DELIMITER]).unwrap();
    assert!(!body.contains(&DELIMITER), "one message, one line");
    assert_eq!(line(&unwritable), [DELIMITER].as_slice());
}
