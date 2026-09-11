//! The identifiers a seeded fleet is given, and why the schema accepts them.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{KIND_FLEET, KIND_TENANT, KIND_WORKSPACE, identifier};
use crate::RunPrefix;

fn prefix(value: &str) -> RunPrefix {
    RunPrefix::existing(value).expect("the test prefix is valid")
}

fn id(prefix: &RunPrefix, kind: u32, index: u64) -> String {
    identifier(prefix, kind, index).expect("the fixed timestamp encodes")
}

#[test]
fn test_an_identifier_passes_the_schemas_version_check() {
    let id = id(&prefix("bench-123-1"), KIND_FLEET, 0);

    let groups: Vec<&str> = id.split('-').collect();
    assert_eq!(groups.len(), 5, "a UUID has five groups, got {id}");
    assert_eq!(
        groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
        vec![8, 4, 4, 4, 12]
    );
    assert!(
        groups
            .get(2)
            .is_some_and(|version| version.starts_with('7')),
        "the character after the second dash must be 7 or `core.fleets` refuses the row: {id}"
    );
    assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
}

#[test]
fn test_the_three_kinds_never_collide_for_one_index() {
    let (t, w, f) = (
        id(&prefix("bench-123-1"), KIND_TENANT, 9),
        id(&prefix("bench-123-1"), KIND_WORKSPACE, 9),
        id(&prefix("bench-123-1"), KIND_FLEET, 9),
    );
    assert_ne!(t, w);
    assert_ne!(w, f);
    assert_ne!(t, f);
}

#[test]
fn test_two_indexes_never_collide_within_a_kind() {
    let run = prefix("bench-123-1");
    assert_ne!(id(&run, KIND_FLEET, 1), id(&run, KIND_FLEET, 2));
    assert_ne!(
        id(&run, KIND_FLEET, 0),
        id(&run, KIND_FLEET, u32::MAX.into())
    );
}

#[test]
fn test_two_runs_never_adopt_the_same_fixture_rows() {
    assert_ne!(
        id(&prefix("bench-123-1"), KIND_FLEET, 7),
        id(&prefix("bench-123-10"), KIND_FLEET, 7),
    );
}
