//! Each scripted choice depends on a result or instruction the model received.
use crate::fake_model::{Asked, FakeModel, Transcript, call, say};
use serde_json::json;

pub(super) const ORIGIN: &str = "docs.vendor.example";
pub(super) const DEST: &str = "support.vendor.example";
pub(super) const GUIDE: &str = "https://docs.vendor.example/guide";
pub(super) const SENTINEL: &str = "Migration begins with checkpoint cobalt-487.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Retry,
    DeclineRetry,
    Denied,
}

impl Outcome {
    pub(super) const fn answer(self) -> &'static str {
        match self {
            Self::Retry => SENTINEL,
            Self::DeclineRetry => {
                "I received approval and skipped the retry. I have not read the guide."
            }
            Self::Denied => {
                "Access to support.vendor.example was denied, so I could not read the guide."
            }
        }
    }
}

fn instruction(asked: &Asked, expected: &str) -> bool {
    asked
        .results
        .iter()
        .chain(&asked.user)
        .any(|text| text.contains(expected))
}

fn read_requested_guide(asked: &Asked, id: &str) -> guffbolt::Chunk {
    let url = asked
        .user
        .iter()
        .flat_map(|message| message.split_whitespace())
        .find(|word| word.starts_with("https://"));
    url.map_or_else(
        || say("The original guide URL is missing."),
        |url| call(id, "web_fetch", json!({"url":url})),
    )
}

pub(super) fn model(outcome: Outcome) -> (FakeModel, Transcript) {
    FakeModel::deciding(move |asked| {
        let reply = if asked
            .results
            .last()
            .is_some_and(|result| result.contains(SENTINEL))
        {
            say(SENTINEL)
        } else if instruction(asked, "Retry web_fetch") {
            if outcome == Outcome::Retry {
                read_requested_guide(asked, "retry-guide")
            } else {
                say(Outcome::DeclineRetry.answer())
            }
        } else if instruction(asked, "Explain why web_fetch could not proceed") {
            say(Outcome::Denied.answer())
        } else if asked
            .results
            .last()
            .is_some_and(|result| result.contains(DEST) && result.contains("request_access"))
        {
            call("ask-host", "request_access", json!({"host":DEST}))
        } else if asked.turn == 0 && instruction(asked, GUIDE) {
            read_requested_guide(asked, "read-guide")
        } else {
            say("Unexpected script input.")
        };
        vec![reply]
    })
}

pub(super) fn assert_continuation(asked: &[Asked], outcome: Outcome) {
    let decision = if outcome == Outcome::Denied {
        "denied"
    } else {
        "approved"
    };
    let instruction = format!("Access to {DEST} {decision}.");
    assert!(
        asked
            .iter()
            .any(|turn| self::instruction(turn, &instruction)),
        "{asked:?}"
    );
    assert_eq!(
        asked
            .iter()
            .any(|turn| turn.results.iter().any(|result| result.contains(SENTINEL))),
        outcome == Outcome::Retry
    );
}
