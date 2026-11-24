use af_core::id::Uuid7;

use super::{FleetSeed, INSERT_FLEET};

const ID: &str = "0199f0c0-0000-7000-8000-000000000001";
const WORKSPACE: &str = "0199f0c0-0000-7000-8000-000000000002";
const TENANT: &str = "0199f0c0-0000-7000-8000-000000000003";
const STATUS: &str = "active";

/// When the seeded fleet was created, and when it was last updated.
const CREATED: jiff::Timestamp = jiff::Timestamp::constant(0, 10_000_000);
const UPDATED: jiff::Timestamp = jiff::Timestamp::constant(0, 20_000_000);

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

/// The three fixed ids, as the seed takes them.
fn ids() -> af_core::error::Result<(Uuid7, Uuid7, Uuid7)> {
    Ok((
        Uuid7::parse(ID)?,
        Uuid7::parse(WORKSPACE)?,
        Uuid7::parse(TENANT)?,
    ))
}

#[test]
fn an_unnamed_fleet_takes_its_id_so_two_seeds_in_a_workspace_never_collide()
-> af_core::error::Result<()> {
    let (id, workspace, tenant) = ids()?;
    let seed = FleetSeed::new(&id, &workspace, &tenant, STATUS);

    assert_eq!(seed.name, ID);
    Ok(())
}

#[test]
fn a_later_update_moves_updated_at_and_leaves_created_at() -> af_core::error::Result<()> {
    let (id, workspace, tenant) = ids()?;
    let seed = FleetSeed::new(&id, &workspace, &tenant, STATUS)
        .at(CREATED)
        .updated(UPDATED);

    assert_eq!(
        (seed.created_at, seed.updated_at),
        (Some(CREATED), Some(UPDATED))
    );
    Ok(())
}

/// The network a seeded fleet takes is spelled as the crate that owns the
/// column's vocabulary spells a new fleet's.
#[test]
fn the_seeded_network_is_a_new_fleets() {
    assert_eq!(
        super::FIXTURE_HTTP_EGRESS,
        af_fleet_runtime::config::HttpEgress::default().as_str()
    );
}

/// A fleet with no named tags takes the unplaced tag, so no other suite's
/// runner is handed its work; one that named its own keeps them.
#[test]
fn an_unplaced_fleet_keeps_named_tags_and_otherwise_takes_the_unplaced_one()
-> af_core::error::Result<()> {
    let (id, workspace, tenant) = ids()?;
    let untagged = FleetSeed::new(&id, &workspace, &tenant, STATUS).unplaced();
    assert_eq!(untagged.required_tags, ["fixture-unplaced"]);

    let tagged = FleetSeed::new(&id, &workspace, &tenant, STATUS)
        .tags(&["linux-runner"])
        .unplaced();
    assert_eq!(tagged.required_tags, ["linux-runner"]);
    Ok(())
}

#[test]
fn a_fresh_seed_runs_the_fixture_model_on_a_restricted_network() -> af_core::error::Result<()> {
    let (id, workspace, tenant) = ids()?;
    let fresh = FleetSeed::new(&id, &workspace, &tenant, STATUS);
    assert_eq!(
        (fresh.model_ref, fresh.http_egress, fresh.package_domains),
        (super::FIXTURE_MODEL_REF, super::FIXTURE_HTTP_EGRESS, false)
    );

    let opened = fresh.model("tenant:kimi").network("public", true);
    assert_eq!(
        (opened.model_ref, opened.http_egress, opened.package_domains),
        ("tenant:kimi", "public", true)
    );
    Ok(())
}
