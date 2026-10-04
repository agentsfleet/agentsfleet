//! A table answers the path a report names, in table order, and nothing else.

use garde::Validate as _;

use super::{PathTable, Sentences};

const HOST_ID: &str = "host_id must be 1-256 chars";
const NOTE: &str = "a bind note must be at most 200 bytes";
const FALLBACK: &str = "the request is malformed";

const PATH_HOST_ID: &str = "host_id";
const PATH_NOTE: &str = "binds[].note";

const TABLE: Sentences = Sentences::new(&[(PATH_HOST_ID, HOST_ID), (PATH_NOTE, NOTE)], FALLBACK);

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

fn register(host_id: &str, notes: &[&str], label: &str) -> Register {
    Register {
        host_id: host_id.to_owned(),
        binds: notes
            .iter()
            .map(|note| Bind {
                note: (*note).to_owned(),
            })
            .collect(),
        label: label.to_owned(),
    }
}

fn pick(value: &Register) -> Option<&'static str> {
    value.validate().err().map(|report| TABLE.pick(&report))
}

#[test]
fn test_sentences_pick_the_reported_path() {
    assert_eq!(pick(&register("", &["ok"], "ok")), Some(HOST_ID));
    assert_eq!(pick(&register("ok", &["too long"], "ok")), Some(NOTE));
    // `label` is not in the table: the fallback answers, not garde's text.
    assert_eq!(pick(&register("ok", &["ok"], "too long")), Some(FALLBACK));
    assert_eq!(pick(&register("ok", &["ok"], "ok")), None);
}

#[test]
fn an_entry_matches_its_field_in_every_element() {
    // The break is in the twelfth bind; the table names the field once.
    let mut notes = vec!["ok"; 11];
    notes.push("too long");
    assert_eq!(pick(&register("ok", &notes, "ok")), Some(NOTE));
}

#[test]
fn table_order_decides_between_two_breaks() {
    assert_eq!(pick(&register("", &["too long"], "ok")), Some(HOST_ID));
    let reversed = Sentences::new(&[(PATH_NOTE, NOTE), (PATH_HOST_ID, HOST_ID)], FALLBACK);
    let report = register("", &["too long"], "ok").validate().err();
    assert_eq!(report.map(|report| reversed.pick(&report)), Some(NOTE));
}

/// A crate that answers with a variant rather than a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rejection {
    Host,
    Note,
    Malformed,
}

#[test]
fn a_table_answers_a_variant_as_readily_as_a_sentence() {
    const VARIANTS: PathTable<Rejection> = PathTable::new(
        &[
            (PATH_HOST_ID, Rejection::Host),
            (PATH_NOTE, Rejection::Note),
        ],
        Rejection::Malformed,
    );
    let answer = |value: &Register| value.validate().err().map(|report| VARIANTS.pick(&report));
    assert_eq!(answer(&register("", &["ok"], "ok")), Some(Rejection::Host));
    assert_eq!(
        answer(&register("ok", &["ok", "too long"], "ok")),
        Some(Rejection::Note)
    );
    assert_eq!(
        answer(&register("ok", &["ok"], "too long")),
        Some(Rejection::Malformed)
    );
}
