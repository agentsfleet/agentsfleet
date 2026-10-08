#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

//! What a read carries, and what an edit refuses to write back.

use afr_executor::MAX_READ_BYTES;
use serde_json::json;

use super::tests::{NEW_TEXT, OLD_TEXT, PATH};
use crate::catalog::{FILE_EDIT, FILE_READ};
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::testing::{Live, call_in, hosted, offered};

/// A file longer than one read carries is read cut to the budget and says
/// where the rest is, and is refused for an edit, which would write it back
/// cut.
#[tokio::test]
async fn a_file_past_one_read_is_cut_for_reading_and_refused_for_editing() {
    let live = Live::start().await;
    let length = usize::try_from(MAX_READ_BYTES).unwrap() + 1;
    std::fs::write(live.root.join("big.txt"), vec![b'x'; length]).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ.name(), FILE_EDIT.name()])
        .unwrap();
    let lease = Lease::default();

    let read = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &lease,
        json!({PATH: "big.txt"}),
    )
    .await;
    let edit = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &lease,
        json!({PATH: "big.txt", OLD_TEXT: "x", NEW_TEXT: "y"}),
    )
    .await;
    let past = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &lease,
        json!({PATH: "big.txt", "offset": 2}),
    )
    .await;

    assert!(
        read.text.ends_with(&format!(
            "... the file continues past {MAX_READ_BYTES} bytes; read the rest with shell"
        )),
        "{}",
        read.text.len()
    );
    assert!(
        read.text.len() < 41_000,
        "one line past the budget is cut to it: {} bytes",
        read.text.len()
    );
    assert_eq!(read.error_code, None);
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::FileTooLarge),
        "{edit:?}"
    );
    assert_eq!(past.error_code, Some(ToolErrorCode::InvalidArguments));
    assert!(
        past.text.ends_with(&format!(
            "of the first {MAX_READ_BYTES} bytes of big.txt; read the rest with shell"
        )),
        "an offset past one read points at shell: {}",
        past.text
    );
    live.stop().await;
}

#[tokio::test]
async fn a_file_that_is_not_text_is_read_lossily_and_refused_for_editing() {
    let live = Live::start().await;
    std::fs::write(live.root.join("blob"), [0xff, 0xfe, b'a']).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[FILE_READ.name(), FILE_EDIT.name()])
        .unwrap();
    let lease = Lease::default();

    let read = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &lease,
        json!({PATH: "blob"}),
    )
    .await;
    let edit = call_in(
        offered(&selection, &FILE_EDIT),
        &live.client,
        &lease,
        json!({PATH: "blob", OLD_TEXT: "a", NEW_TEXT: "b"}),
    )
    .await;

    assert_eq!(read.text, "\u{fffd}\u{fffd}a");
    assert_eq!(
        edit.error_code,
        Some(ToolErrorCode::InvalidArguments),
        "{edit:?}"
    );
    assert!(edit.text.contains("is not text"), "{}", edit.text);
    live.stop().await;
}
