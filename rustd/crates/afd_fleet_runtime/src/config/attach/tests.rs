//! Writing a channel into a document, proven by reading the result back.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use serde_json::Value;

use super::attach_mention;
use crate::config::{Mention, Trigger};
use crate::error::ErrorKind;
use crate::{instructions, parse_trigger};

const SOURCE: &str = "slack";
const CHANNEL: &str = "C0123456789";
const OTHER_CHANNEL: &str = "C0987654321";

/// A document as an author writes one: a comment, a flow-style list, prose.
const AUTHORED: &str = "---\n# who answers in #ci-dev\nname: ci-responder\nx-agentsfleet:\n  triggers: [{type: api}]\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n\nRead the failed run, then answer.\n";

fn mention(channel: &str) -> Mention {
    Mention {
        source: SOURCE.into(),
        channel: channel.parse().expect("a channel identifier"),
    }
}

/// The stored configuration with its trigger list blanked, which is every
/// part of the document an attach must leave alone.
fn without_triggers(config_json: &str) -> Value {
    let mut tree: Value = serde_json::from_str(config_json).expect("stored JSON");
    if let Some(triggers) = tree.pointer_mut("/x-agentsfleet/triggers") {
        *triggers = Value::Null;
    }
    tree
}

/// Dimension 2.4 — the attached document reads back with the channel's
/// mention after the author's own triggers, and nothing else moved: the
/// configuration, its key order and the prose are the author's.
#[test]
fn an_attached_document_reads_back_with_the_channel_and_nothing_else_moved() {
    let attached = attach_mention(AUTHORED, &mention(CHANNEL)).expect("attaches");

    let before = parse_trigger(AUTHORED).expect("the authored document parses");
    let after = parse_trigger(&attached).expect("the attached document parses");
    assert_eq!(
        after.config().triggers(),
        [Trigger::Api, Trigger::Mention(mention(CHANNEL))]
    );
    assert_eq!(
        without_triggers(after.config_json()),
        without_triggers(before.config_json())
    );
    assert!(
        after
            .config_json()
            .starts_with(r#"{"name":"ci-responder","x-agentsfleet":{"triggers""#),
        "authored key order is kept: {}",
        after.config_json()
    );
    assert_eq!(instructions(&attached), instructions(AUTHORED));
}

/// Attaching the channel a document already names changes no byte, so a
/// retried install and a document attached by hand both store what they had.
#[test]
fn attaching_the_same_channel_twice_changes_nothing() {
    let once = attach_mention(AUTHORED, &mention(CHANNEL)).expect("attaches");
    let twice = attach_mention(&once, &mention(CHANNEL)).expect("attaches again");
    assert_eq!(twice, once);
}

/// A fleet already attached to one channel is not quietly moved to a second:
/// the result is a document the parser refuses, naming the duplicate.
#[test]
fn a_second_channel_is_refused_rather_than_added() {
    let once = attach_mention(AUTHORED, &mention(CHANNEL)).expect("attaches");
    let both = attach_mention(&once, &mention(OTHER_CHANNEL)).expect("writes");
    let refused = parse_trigger(&both).expect_err("two channels are refused");
    assert!(
        matches!(refused.kind(), ErrorKind::InvalidTriggerSet { .. }),
        "{refused}"
    );
}

/// A document with no prose keeps none, and one with no frontmatter is the
/// parser's refusal, not a document this writes.
#[test]
fn the_edges_of_a_document_are_the_parsers() {
    let bare = "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n";
    let attached = attach_mention(bare, &mention(CHANNEL)).expect("attaches");
    assert!(attached.ends_with("---\n"), "{attached}");
    assert_eq!(instructions(&attached), "");

    let refused = attach_mention("Just prose.\n", &mention(CHANNEL)).expect_err("no block");
    assert!(matches!(refused.kind(), ErrorKind::FrontmatterMissing));
}
