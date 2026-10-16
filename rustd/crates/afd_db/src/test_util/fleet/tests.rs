use super::{FleetSeed, INSERT_FLEET};

const ID: &str = "0199f0c0-0000-7000-8000-000000000001";
const WORKSPACE: &str = "0199f0c0-0000-7000-8000-000000000002";
const TENANT: &str = "0199f0c0-0000-7000-8000-000000000003";
const STATUS: &str = "active";

/// The text between `open` and the next `close`, when the statement has both.
fn between<'a>(text: &'a str, open: &str, close: char) -> Option<&'a str> {
    let (_, rest) = text.split_once(open)?;
    rest.split_once(close).map(|(inside, _)| inside)
}

/// The number each `$N` value carries, skipping any value that is not one.
fn placeholder(value: &str) -> Option<usize> {
    let digits: String = value
        .trim()
        .strip_prefix('$')?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[test]
fn every_named_column_has_exactly_one_placeholder_in_order() {
    let columns = between(INSERT_FLEET, "core.fleets (", ')').map(|list| list.split(',').count());
    let placeholders: Option<Vec<usize>> = between(INSERT_FLEET, "VALUES (", ')')
        .map(|values| values.split(',').filter_map(placeholder).collect());

    assert!(columns.is_some_and(|count| count > 0), "{INSERT_FLEET}");
    assert_eq!(
        placeholders,
        columns.map(|count| (1..=count).collect()),
        "{INSERT_FLEET}"
    );
}

#[test]
fn an_unnamed_fleet_takes_its_id_so_two_seeds_in_a_workspace_never_collide() {
    let seed = FleetSeed::new(ID, WORKSPACE, TENANT, STATUS);

    assert_eq!(seed.name, ID);
}

#[test]
fn a_later_update_moves_updated_at_and_leaves_created_at() {
    let seed = FleetSeed::new(ID, WORKSPACE, TENANT, STATUS)
        .at(10)
        .updated(20);

    assert_eq!((seed.created_at, seed.updated_at), (10, 20));
}
