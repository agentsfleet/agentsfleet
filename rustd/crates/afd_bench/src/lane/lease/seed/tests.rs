//! The identifiers a seeded fleet is given, and why the schema accepts them.

use super::{KIND_FLEET, KIND_TENANT, KIND_WORKSPACE, identifier};

#[test]
fn test_an_identifier_passes_the_schemas_version_check() {
    let id = identifier(KIND_FLEET, 0);

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
        identifier(KIND_TENANT, 9),
        identifier(KIND_WORKSPACE, 9),
        identifier(KIND_FLEET, 9),
    );
    assert_ne!(t, w);
    assert_ne!(w, f);
    assert_ne!(t, f);
}

#[test]
fn test_two_indexes_never_collide_within_a_kind() {
    assert_ne!(identifier(KIND_FLEET, 1), identifier(KIND_FLEET, 2));
    assert_ne!(
        identifier(KIND_FLEET, 0),
        identifier(KIND_FLEET, u32::MAX.into())
    );
}
