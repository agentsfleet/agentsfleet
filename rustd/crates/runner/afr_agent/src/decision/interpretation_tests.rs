//! Every independent request survives the decomposition boundary intact.

use super::{interpretation::validate, routing::Multiplicity};
use af_wire::task::{TaskInputAction, TaskProposedItem};

fn item(start: i32, end: i32) -> TaskProposedItem {
    TaskProposedItem {
        source_start: start,
        source_end: end,
        action: TaskInputAction::Conversation,
        task_id: None,
        title: "Reply to the request".to_owned(),
        interpretation: "Answer this complete source item".to_owned(),
        target: None,
    }
}

#[test]
fn decomposition_preserves_utf8_boundaries_order_and_all_non_separator_text() -> crate::Result<()> {
    let message = "hi  réparé";
    validate(&[item(0, 2), item(4, 12)], message, Multiplicity::Multiple)?;
    assert!(
        validate(&[item(0, 2)], message, Multiplicity::Single).is_err(),
        "cannot drop the actual work after a greeting"
    );
    assert!(
        validate(&[item(0, 2), item(6, 12)], message, Multiplicity::Multiple).is_err(),
        "cannot split a UTF-8 code point"
    );
    assert!(validate(&[item(4, 12), item(0, 2)], message, Multiplicity::Multiple).is_err());
    assert!(validate(&[item(0, 2), item(1, 12)], message, Multiplicity::Multiple).is_err());
    assert!(validate(&[], message, Multiplicity::Single).is_err());
    assert!(
        validate(&[item(0, 12)], message, Multiplicity::Multiple).is_err(),
        "multiplicity is a recorded constraint"
    );
    Ok(())
}

#[test]
fn an_item_outside_its_declared_bounds_is_refused_before_its_offsets_are_read() {
    let message = "hi  réparé";
    let mut untitled = item(0, 12);
    untitled.title = String::new();
    let before = item(-1, 12);

    for refused in [untitled, before] {
        let failure = validate(&[refused], message, Multiplicity::Single).err();

        assert_eq!(
            failure.as_ref().map(crate::Error::code),
            Some(af_core::error_code::INTERNAL_OPERATION_FAILED)
        );
        assert!(failure.is_some_and(|failure| {
            failure
                .to_string()
                .contains("durable task boundary failed: task decision input or deadline")
        }),);
    }
}
