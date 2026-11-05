//! Every independent request survives the decomposition boundary intact.

use afd_wire::task::{TaskInputAction, TaskProposedItem};
use super::{interpretation::validate, routing::Multiplicity};

fn item(start: i32, end: i32) -> TaskProposedItem {
    TaskProposedItem { source_start: start, source_end: end, action: TaskInputAction::Conversation,
        task_id: None, title: "Reply to the request".to_owned(), interpretation: "Answer this complete source item".to_owned(), target: None }
}

#[test]
fn decomposition_preserves_utf8_boundaries_order_and_all_non_separator_text() {
    let message = "hi  réparé";
    assert!(validate(&[item(0, 2), item(4, 12)], message, Multiplicity::Multiple).is_ok());
    assert!(validate(&[item(0, 2)], message, Multiplicity::Single).is_err(), "cannot drop the actual work after a greeting");
    assert!(validate(&[item(0, 2), item(6, 12)], message, Multiplicity::Multiple).is_err(), "cannot split a UTF-8 code point");
    assert!(validate(&[item(4, 12), item(0, 2)], message, Multiplicity::Multiple).is_err());
    assert!(validate(&[item(0, 2), item(1, 12)], message, Multiplicity::Multiple).is_err());
    assert!(validate(&[], message, Multiplicity::Single).is_err());
    assert!(validate(&[item(0, 12)], message, Multiplicity::Multiple).is_err(), "multiplicity is a recorded constraint");
}
