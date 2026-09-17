//! The census cells: every status published, zero for the absent, a gap for
//! the failed.

use opentelemetry::Value;

use super::FleetCensus;
use crate::metrics::instrument::Reading;
use crate::metrics::label::fleet::FleetStatusLabel;
use crate::semconv;

/// The value a reading carries for `status`, as a string.
fn status_of(reading: &Reading) -> String {
    reading
        .attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == semconv::LABEL_STATUS)
        .map_or_else(String::new, |attribute| match &attribute.value {
            Value::String(spelling) => spelling.to_string(),
            other => other.to_string(),
        })
}

/// Readings as `(status, value)` pairs, in publication order.
fn pairs(census: &FleetCensus) -> Vec<(String, u64)> {
    census
        .readings()
        .iter()
        .map(|reading| (status_of(reading), reading.value))
        .collect()
}

#[test]
fn every_status_is_published_and_the_absent_ones_read_zero() {
    let census = FleetCensus::new();
    census.publish(&[
        (FleetStatusLabel::Active, 2),
        (FleetStatusLabel::Paused, 1),
        (FleetStatusLabel::Stopped, 1),
    ]);

    assert_eq!(
        pairs(&census),
        vec![
            ("installing".to_owned(), 0),
            ("active".to_owned(), 2),
            ("paused".to_owned(), 1),
            ("stopped".to_owned(), 1),
            ("killed".to_owned(), 0),
        ],
        "a successful count that omits a status has measured zero of them"
    );
}

#[test]
fn a_withdrawn_census_publishes_no_reading_at_all() {
    let census = FleetCensus::new();
    assert!(
        census.readings().is_empty(),
        "cells nothing has published into observe nothing"
    );

    census.publish(&[(FleetStatusLabel::Active, 3)]);
    assert_eq!(census.readings().len(), FleetStatusLabel::ALL.len());

    census.withdraw();
    assert!(
        census.readings().is_empty(),
        "a failed count leaves a gap, never a zero"
    );
}

#[test]
fn a_status_listed_twice_is_summed_not_overwritten() {
    let census = FleetCensus::new();
    census.publish(&[(FleetStatusLabel::Killed, 1), (FleetStatusLabel::Killed, 4)]);
    let killed = pairs(&census)
        .into_iter()
        .find(|(status, _)| status == "killed")
        .map(|(_, value)| value);
    assert_eq!(killed, Some(5));
}

#[test]
fn the_label_key_is_the_census_column() {
    let census = FleetCensus::new();
    census.publish(&[]);
    for reading in census.readings() {
        assert_eq!(reading.attributes.len(), 1, "one label, `status`");
        assert_eq!(
            reading
                .attributes
                .first()
                .map(|attribute| attribute.key.as_str()),
            Some(semconv::LABEL_STATUS)
        );
    }
}
