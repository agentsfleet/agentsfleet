#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]
use afd_core::paging::{Boundary as _, Cursor, SortOrder as _};

use super::{ApiKeySort, KeyRow};

/// When the boundary row was minted.
const MINTED: jiff::Timestamp = jiff::Timestamp::constant(1_724_800_000, 0);

/// A row at the end of a page, with a name and an instant that differ.
fn boundary_row() -> KeyRow {
    KeyRow {
        id: "0195b4ba-8d3a-7f13-8abc-2b3e1e0f7031".to_owned(),
        name: "zeta-deploy".to_owned(),
        created_at: MINTED,
        revoked_at: None,
    }
}

/// Every ordering emits the form its own seek can resume from.
///
/// The regression this pins: the rendering used to live in a private
/// handler helper that emitted the timestamp form unconditionally, so a
/// `key_name` walk handed back a cursor the paging layer refuses on the
/// next request — page two never arrived, and nothing failed loudly because
/// the refusal reads as a client sending something malformed.
#[test]
fn a_cursor_carries_the_boundary_its_own_sort_seeks_on() {
    let row = boundary_row();
    for sort in [
        ApiKeySort::CreatedAscending,
        ApiKeySort::CreatedDescending,
        ApiKeySort::NameAscending,
        ApiKeySort::NameDescending,
    ] {
        assert_eq!(
            row.cursor(sort).kind(),
            sort.boundary(),
            "{sort:?} orders by one column and its cursor must name that column"
        );
    }
}

/// A name-ordered cursor survives the round trip a second request makes.
///
/// Rendering the right FORM is only half of it: the value has to come back
/// intact, because the seek compares it against `key_name` directly. A name
/// is caller-supplied text, so the encoding is what has to hold.
#[test]
fn a_name_cursor_round_trips_through_the_wire() {
    let row = boundary_row();
    let rendered = row.cursor(ApiKeySort::NameAscending).to_string();
    let parsed = Cursor::parse(&rendered).expect("a cursor this daemon issued must parse");

    match parsed {
        Cursor::Text { value, id } => {
            assert_eq!(value, row.name, "the boundary name must survive the trip");
            assert_eq!(id, row.id, "and the tiebreak id with it");
        }
        Cursor::Timestamp { .. } => {
            panic!("a name walk must not resume from an instant")
        }
    }
}

/// A row whose columns are not this daemon's shape stays a query fault.
///
/// The reader decodes by name, so a renamed or retyped column surfaces
/// here rather than as a wrong value further in — which is the whole point
/// of routing it through `error::query` with a context instead of letting a
/// bare `sqlx::Error` reach a caller that cannot say which read produced it.
#[test]
fn an_unreadable_api_key_row_keeps_its_context_and_cause() {
    use std::error::Error as _;

    let failure = super::row_unreadable(sqlx::Error::PoolClosed);

    assert!(failure.source().is_some(), "the sqlx cause survives");
    assert_ne!(failure.to_string(), "");
    assert_ne!(failure.code().as_str(), "");
}

/// A key authenticates until it is revoked, and the revocation stamp is the
/// only fact that says so.
#[test]
fn a_key_is_active_until_its_revocation_is_stamped() {
    let live = boundary_row();
    let revoked = KeyRow {
        revoked_at: Some(MINTED),
        ..boundary_row()
    };

    assert!(live.active());
    assert!(!revoked.active());
}
