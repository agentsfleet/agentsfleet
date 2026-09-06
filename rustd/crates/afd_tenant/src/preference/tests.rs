use super::*;

#[test]
fn every_registry_key_round_trips_through_its_wire_spelling() {
    for key in [
        PrefKey::GettingStartedDismissed,
        PrefKey::GettingStartedCollapsed,
        PrefKey::GettingStartedCliTicked,
    ] {
        assert_eq!(PrefKey::parse(key.as_str()), Some(key));
    }
}

#[test]
fn a_key_outside_the_registry_is_refused() {
    // The whole point of the closed registry: an unbounded key space is an
    // unbounded number of rows per user, in a column nobody validates.
    assert_eq!(PrefKey::parse("getting_started"), None);
    assert_eq!(PrefKey::parse(""), None);
    assert_eq!(PrefKey::parse("GETTING_STARTED_DISMISSED"), None);
}

#[test]
fn only_the_json_literal_true_reads_as_ticked() {
    let bag = |value: &str| {
        vec![Pref {
            key: PrefKey::GettingStartedCliTicked.as_str().to_owned(),
            value: value.to_owned(),
        }]
    };

    assert!(bag_is_true(&bag("true"), PrefKey::GettingStartedCliTicked));
    assert!(bag_is_true(
        &bag("  true  "),
        PrefKey::GettingStartedCliTicked
    ));
    // Everything a coercing implementation would accept, refused: the value
    // is stored verbatim, so these are what a client actually wrote.
    for written in ["false", "\"true\"", "1", "TRUE", "null", ""] {
        assert!(
            !bag_is_true(&bag(written), PrefKey::GettingStartedCliTicked),
            "{written} must not read as ticked"
        );
    }
}

#[test]
fn an_absent_key_is_not_ticked() {
    assert!(!bag_is_true(&[], PrefKey::GettingStartedDismissed));
    assert!(!bag_is_true(
        &[Pref {
            key: PrefKey::GettingStartedCollapsed.as_str().to_owned(),
            value: JSON_TRUE.to_owned(),
        }],
        PrefKey::GettingStartedDismissed
    ));
}
