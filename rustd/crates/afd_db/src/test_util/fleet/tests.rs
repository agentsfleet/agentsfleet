use super::{FleetSeed, INSERT_FLEET};

const ID: &str = "0199f0c0-0000-7000-8000-000000000001";
const WORKSPACE: &str = "0199f0c0-0000-7000-8000-000000000002";
const TENANT: &str = "0199f0c0-0000-7000-8000-000000000003";
const STATUS: &str = "active";

/// The instant `millis` after the epoch, for a fixture.
fn instant(millis: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_millisecond(millis).unwrap_or(jiff::Timestamp::UNIX_EPOCH)
}

/// The text between `open` and the next `close`, when the statement has both.
fn between<'a>(text: &'a str, open: &str, close: char) -> Option<&'a str> {
    let (_, rest) = text.split_once(open)?;
    rest.split_once(close).map(|(inside, _)| inside)
}

/// Each distinct `$N` in `text`, in the order it first appears.
fn placeholders(text: &str) -> Vec<usize> {
    let mut seen = Vec::new();
    for piece in text.split('$').skip(1) {
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(number) = digits.parse::<usize>()
            && !seen.contains(&number)
        {
            seen.push(number);
        }
    }
    seen
}

#[test]
fn every_named_column_has_exactly_one_placeholder_in_order() {
    let columns = between(INSERT_FLEET, "fleet.fleets (", ')').map(|list| list.split(',').count());
    let values = INSERT_FLEET
        .split_once("VALUES (")
        .map(|(_, rest)| placeholders(rest));

    assert!(columns.is_some_and(|count| count > 0), "{INSERT_FLEET}");
    assert_eq!(
        values,
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
        .at(instant(10))
        .updated(instant(20));

    assert_eq!(
        (seed.created_at, seed.updated_at),
        (Some(instant(10)), Some(instant(20)))
    );
}
