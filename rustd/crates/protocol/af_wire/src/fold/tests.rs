//! The fold request refuses a negative cursor, and both shapes keep their
//! spelling on the wire.

use garde::Validate as _;

use super::{FoldRequest, FoldResponse, FoldedInput};

#[test]
fn a_negative_cursor_is_refused() -> Result<(), garde::Report> {
    let request = FoldRequest {
        fencing_token: 4,
        after_sequence: -1,
    };
    assert!(
        request.validate().is_err(),
        "a negative cursor must be refused"
    );
    FoldRequest {
        after_sequence: 0,
        ..request
    }
    .validate()
}

#[test]
fn a_reply_keeps_its_spelling_and_its_order() -> Result<(), serde_json::Error> {
    let reply = FoldResponse {
        inputs: vec![
            FoldedInput {
                event_id: "e1".into(),
                sequence: 7,
                text: "use staging".into(),
            },
            FoldedInput {
                event_id: "e2".into(),
                sequence: 9,
                text: "and tag it".into(),
            },
        ],
    };
    let json = serde_json::to_string(&reply)?;
    assert_eq!(
        json,
        r#"{"inputs":[{"event_id":"e1","sequence":7,"text":"use staging"},{"event_id":"e2","sequence":9,"text":"and tag it"}]}"#
    );
    assert_eq!(serde_json::from_str::<FoldResponse<'_>>(&json)?, reply);
    Ok(())
}
