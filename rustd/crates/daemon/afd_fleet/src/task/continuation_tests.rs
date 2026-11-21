//! A continuation inherits its source's destination unless one is recorded,
//! and a recorded destination names a known connector and an address together.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on a malformed fixture identifier"
)]

use af_core::id::Uuid7;
use afd_turns::Reply;

use super::Continuation;
use crate::{
    error::ErrorKind,
    task::{TaskRefusal, TaskScope},
};

const TASK: &str = "01924f4e-0000-7000-8000-0000000000a1";
const SOURCE: &str = "01924f4e-0000-7000-8000-0000000000b2";

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("fixture identifier")
}

fn continuation() -> Continuation {
    let scope = TaskScope {
        workspace_id: id("01924f4e-0000-7000-8000-000000000001"),
        fleet_id: id("01924f4e-0000-7000-8000-00000000fee7"),
    };
    Continuation::new(&scope, &id(TASK), 4, &id(SOURCE))
}

/// The refusal `with_reply` answers, `None` when it accepts or fails otherwise.
fn refused(provider: Option<&str>, address: Option<&str>) -> Option<TaskRefusal> {
    match continuation().with_reply(provider, address) {
        Ok(_accepted) => None,
        Err(error) => match error.kind() {
            ErrorKind::TaskRefused { reason } => Some(*reason),
            _ => None,
        },
    }
}

#[test]
fn an_unrecorded_destination_is_inherited_from_the_source_event() {
    let continuation = continuation();
    let turn = continuation.turn();
    let source = id(SOURCE);

    assert_eq!(turn.reply, Reply::Inherit { turn_id: &source });
    assert_eq!(turn.actor, "task:01924f4e-0000-7000-8000-0000000000a1");
}

#[test]
fn a_recorded_destination_names_its_connector_and_address() {
    let recorded = continuation()
        .with_reply(Some("slack"), Some("C0123456789"))
        .expect("a known connector with an address is a destination");

    assert_eq!(
        recorded.turn().reply,
        Reply::To {
            connector: "slack",
            address: "C0123456789",
        }
    );
}

#[test]
fn a_cleared_destination_owes_no_reply() {
    let cleared = continuation()
        .with_reply(None, None)
        .expect("no provider and no address is no destination");

    assert_eq!(cleared.turn().reply, Reply::None);
}

#[test]
fn an_unknown_connector_or_a_half_destination_is_an_invalid_target() {
    assert_eq!(
        refused(Some("carrier-pigeon"), Some("C0123456789")),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(
        refused(Some("slack"), None),
        Some(TaskRefusal::InvalidTarget)
    );
    assert_eq!(
        refused(None, Some("C0123456789")),
        Some(TaskRefusal::InvalidTarget)
    );
}
