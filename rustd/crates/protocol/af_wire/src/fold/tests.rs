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
                turn_id: "e1".into(),
                sequence: 7,
                text: "use staging".into(),
            },
            FoldedInput {
                turn_id: "e2".into(),
                sequence: 9,
                text: "and tag it".into(),
            },
        ],
    };
    let json = serde_json::to_string(&reply)?;
    assert_eq!(
        json,
        r#"{"inputs":[{"turn_id":"e1","sequence":7,"text":"use staging"},{"turn_id":"e2","sequence":9,"text":"and tag it"}]}"#
    );
    assert_eq!(serde_json::from_str::<FoldResponse<'_>>(&json)?, reply);
    Ok(())
}

fn inputs(texts: &[&'static str]) -> FoldResponse<'static> {
    FoldResponse {
        inputs: (1..)
            .zip(texts)
            .map(|(sequence, text)| FoldedInput {
                turn_id: "e".into(),
                sequence,
                text: (*text).into(),
            })
            .collect(),
    }
}

/// The oldest inputs that fit are kept, measured as the reply serializes,
/// and one byte less drops the last of them.
#[test]
fn a_reply_keeps_the_oldest_inputs_that_fit_its_bytes() -> Result<(), serde_json::Error> {
    let whole = inputs(&["one", "two", "three"]);
    let two = serde_json::to_string(&inputs(&["one", "two"]))?.len();
    for (bytes_max, kept) in [(two, 2), (two - 1, 1), (usize::MAX, 3)] {
        let mut reply = whole.clone();
        reply.keep_within(bytes_max)?;
        assert_eq!(
            Some(reply.inputs.as_slice()),
            whole.inputs.get(..kept),
            "{bytes_max}"
        );
        assert!(kept == 1 || serde_json::to_string(&reply)?.len() <= bytes_max);
    }
    Ok(())
}

/// A first input larger than the room is still sent, alone: a run is never
/// answered with nothing while a message waits.
#[test]
fn a_reply_always_keeps_its_first_input() -> Result<(), serde_json::Error> {
    let mut reply = inputs(&["far too long for the room", "next"]);
    reply.keep_within(1)?;
    assert_eq!(
        reply
            .inputs
            .iter()
            .map(|input| input.sequence)
            .collect::<Vec<_>>(),
        [1]
    );
    let mut empty = FoldResponse::default();
    empty.keep_within(1)?;
    assert_eq!(empty, FoldResponse::default());
    Ok(())
}
