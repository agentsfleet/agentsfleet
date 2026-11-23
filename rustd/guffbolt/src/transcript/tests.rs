//! The transcript joins input so no provider sees two user messages in a
//! row, and knows where the current group began.

use guffbolt_ai::{Message, Replay};

use super::Transcript;

fn said(text: &str) -> Message {
    Message::Assistant {
        text: text.to_owned(),
        calls: Vec::new(),
        replay: Replay::default(),
    }
}

#[test]
fn input_joins_a_waiting_user_message_and_opens_one_after_an_answer() {
    let mut transcript = Transcript::new(vec![Message::User("deploy the fix".to_owned())]);
    transcript.join("use staging".to_owned());
    transcript.push(said("Deployed to staging."));
    transcript.join("thanks".to_owned());
    assert_eq!(
        transcript.messages(),
        [
            Message::User("deploy the fix\n\nuse staging".to_owned()),
            said("Deployed to staging."),
            Message::User("thanks".to_owned()),
        ]
    );
}

#[test]
fn a_group_spans_what_arrived_since_it_began() {
    let mut opened = Transcript::new(vec![Message::User("deploy the fix".to_owned())]);
    assert!(opened.group().is_empty(), "the opening is no group's");
    opened.begin_group();
    opened.push(said("Deployed."));
    assert_eq!(opened.group(), [said("Deployed.")]);

    let resumed = Transcript::resumed(vec![Message::User("resume".to_owned()), said("Done.")]);
    assert_eq!(
        resumed.group().len(),
        2,
        "a resumed history is all one group"
    );
}
