#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::collections::BTreeMap;

use bytes::Bytes;
use serde_json::value::RawValue;

use super::{DELIMITER, ExitedParams, WriteParams, decoded, line};

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

#[test]
fn a_non_string_bytes_field_explains_the_expected_encoding() {
    let refused = serde_json::from_str::<WriteParams>(r#"{"process_id":1,"data":5}"#).unwrap_err();
    assert!(refused.is_data(), "{refused}");
    assert!(
        refused.to_string().contains("expected standard base64"),
        "{refused}"
    );
}

use super::{MAX_FRAME_BYTES, MAX_READ_BYTES};

/// The read cap is the contract a handler reads a whole file under, and as
/// base64 it fits one frame.
#[test]
fn the_read_cap_is_eight_mebibytes_and_fits_one_frame_as_base64() {
    // pin test: literal is the contract
    assert_eq!(MAX_READ_BYTES, 8 * 1024 * 1024);
    assert!(usize::try_from(MAX_READ_BYTES).unwrap() / 3 * 4 < MAX_FRAME_BYTES);
}

/// An exit from an executor that never measured whether output was left
/// behind reads as not abandoned: a type the runner reads stays lenient.
#[test]
fn an_exit_without_output_abandoned_decodes_as_not_abandoned() {
    let raw: Box<RawValue> =
        serde_json::from_str(r#"{"process_id":7,"ending":{"kind":"exited","code":0}}"#).unwrap();

    let exited: ExitedParams = decoded(&raw).unwrap();

    assert!(!exited.output_abandoned);
    assert_eq!(exited.process_id, 7);
}
