//! A table answers the path a report names, in table order, and nothing else.

use garde::Validate as _;

use super::Sentences;

const HOST_ID: &str = "host_id must be 1-256 chars";
const NOTE: &str = "a bind note must be at most 200 bytes";
const FALLBACK: &str = "the request is malformed";

const TABLE: Sentences = Sentences::new(&[("host_id", HOST_ID), ("binds[0].note", NOTE)], FALLBACK);

#[derive(Debug, garde::Validate)]
struct Bind {
    #[garde(length(bytes, max = 4))]
    note: String,
}

#[derive(Debug, garde::Validate)]
struct Register {
    #[garde(length(bytes, min = 1, max = 4))]
    host_id: String,
    #[garde(dive)]
    binds: Vec<Bind>,
    #[garde(length(bytes, max = 4))]
    label: String,
}

fn register(host_id: &str, note: &str, label: &str) -> Register {
    Register {
        host_id: host_id.to_owned(),
        binds: vec![Bind {
            note: note.to_owned(),
        }],
        label: label.to_owned(),
    }
}

fn pick(value: &Register) -> Option<&'static str> {
    value.validate().err().map(|report| TABLE.pick(&report))
}

#[test]
fn test_sentences_pick_the_reported_path() {
    assert_eq!(pick(&register("", "ok", "ok")), Some(HOST_ID));
    assert_eq!(pick(&register("ok", "too long", "ok")), Some(NOTE));
    // `label` is not in the table: the fallback answers, not garde's text.
    assert_eq!(pick(&register("ok", "ok", "too long")), Some(FALLBACK));
    assert_eq!(pick(&register("ok", "ok", "ok")), None);
}

#[test]
fn table_order_decides_between_two_breaks() {
    // Both bounds broke; the table lists `host_id` first, so it answers even
    // though the report would name them in another order were they swapped.
    assert_eq!(pick(&register("", "too long", "ok")), Some(HOST_ID));
    let reversed = Sentences::new(&[("binds[0].note", NOTE), ("host_id", HOST_ID)], FALLBACK);
    let report = register("", "too long", "ok").validate().err();
    assert_eq!(report.map(|report| reversed.pick(&report)), Some(NOTE));
}
