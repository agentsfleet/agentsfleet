//! How destinations are scripted from the requested fractions.

#![expect(
    clippy::indexing_slicing,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use core::time::Duration;
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use afd_db::test_util::mint_id;
use afd_outbound::Deliver as _;
use afd_outbound::Outgoing;

use super::poster::{Behaviour, Scripted};
use super::record::{Drained, record, window_end};
use super::{DESTINATIONS, Parameters, fraction_of, script};
use crate::error::Result;
use crate::fixture::RunPrefix;
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

/// The thread an owed answer is addressed to, as a Slack producer records it.
const DESTINATION: &str = r#"{"channel_id":"C0123456789","thread_ts":"1700000000.000100"}"#;

fn parameters(slow: f64, retryable: f64) -> Parameters {
    Parameters {
        jobs: 1,
        slow_fraction: slow,
        retryable_fraction: retryable,
        window: Duration::from_secs(1),
    }
}

#[test]
fn test_one_in_sixteen_is_exactly_one_slow_destination() -> Result<()> {
    let scripted = script(&RunPrefix::mint(), parameters(0.0625, 0.0))?;

    let slow = scripted.values().filter(|b| **b == Behaviour::Slow).count();
    assert_eq!(slow, 1);
    assert_eq!(
        scripted.len() as u64,
        DESTINATIONS,
        "every destination is scripted"
    );
    Ok(())
}

#[test]
fn test_slow_and_retryable_populations_do_not_overlap() -> Result<()> {
    let scripted = script(&RunPrefix::mint(), parameters(0.25, 0.25))?;

    let slow = scripted.values().filter(|b| **b == Behaviour::Slow).count();
    let retryable = scripted
        .values()
        .filter(|b| **b == Behaviour::Retryable)
        .count();
    let fast = scripted.values().filter(|b| **b == Behaviour::Fast).count();
    assert_eq!((slow, retryable, fast), (4, 4, 8));
    Ok(())
}

#[test]
fn test_a_fraction_past_one_is_clamped_and_below_zero_selects_nothing() {
    assert_eq!(fraction_of(DESTINATIONS, 7.0), DESTINATIONS);
    assert_eq!(fraction_of(DESTINATIONS, -1.0), 0);
    assert_eq!(
        fraction_of(DESTINATIONS, f64::NAN),
        0,
        "NaN clamps to the floor, not to a panic"
    );
}

#[test]
fn test_a_fraction_that_selects_less_than_one_destination_selects_none() {
    assert_eq!(
        fraction_of(DESTINATIONS, 0.01),
        0,
        "rounded down, so a run does not get a slow destination it did not ask for"
    );
}

#[test]
fn test_two_scripts_of_one_prefix_deal_destinations_in_the_same_order() -> Result<()> {
    let prefix = RunPrefix::mint();
    let first: Vec<String> = script(&prefix, parameters(0.25, 0.0))?
        .into_keys()
        .collect();
    let second: Vec<String> = script(&prefix, parameters(0.25, 0.0))?
        .into_keys()
        .collect();
    assert_eq!(
        first, second,
        "the interleaving is a function of the parameters, not a hash seed"
    );
    assert!(
        first.windows(2).all(|pair| pair[0] < pair[1]),
        "and it is sorted"
    );
    Ok(())
}

/// The window closes when the last answer lands, not when the worker asked.
///
/// The first version stamped the attempt and counted it settled before the
/// scripted delay elapsed, so the slow lane's denominator lost its last 250 ms.
/// The delay here is real and short: the equality below is exact whatever the
/// clock did, and the count is read on either side of the await.
#[tokio::test]
async fn test_the_window_ends_when_the_last_answer_lands_not_when_it_is_asked_for() {
    let slow = Duration::from_millis(20);
    let fleet = mint_id();
    let mut behaviours = BTreeMap::new();
    behaviours.insert(fleet.to_string(), Behaviour::Slow);
    let poster = Scripted::new(behaviours, Duration::ZERO, slow);
    let job = Outgoing {
        provider: "slack".to_owned(),
        destination: DESTINATION.to_owned(),
        workspace_id: mint_id(),
        fleet_id: fleet,
        event_id: mint_id(),
        answer: "a".to_owned(),
    };

    let answer = poster.deliver(&job);
    assert_eq!(poster.settled(), 0, "asked is not yet answered");
    answer.await;
    assert_eq!(poster.settled(), 1, "the answer is what settles it");

    let seen = poster.seen();
    let asked = seen.attempts()[&job.event_id][0].at;
    assert_eq!(
        window_end(&seen, asked).saturating_duration_since(asked),
        slow,
        "the window closes when the slow answer lands"
    );
}

/// The latency clock and the poster's stamps meet on the event each job answers.
///
/// The lane once keyed the clock by obligation row while the poster stamped by
/// event, so no lookup matched and the report silently carried no p95 and no
/// head-of-line number. Two jobs owed and delivered here must both be timed.
#[tokio::test]
async fn test_a_drained_job_is_timed_by_the_event_the_poster_stamped() -> Result<()> {
    let fleet = mint_id();
    let mut behaviours = BTreeMap::new();
    behaviours.insert(fleet.to_string(), Behaviour::Fast);
    let poster = Scripted::new(behaviours.clone(), Duration::ZERO, Duration::ZERO);
    let started = Instant::now();
    let mut queued_at = HashMap::new();
    for _ in 0..2 {
        let job = Outgoing {
            provider: "slack".to_owned(),
            destination: DESTINATION.to_owned(),
            workspace_id: mint_id(),
            fleet_id: fleet.clone(),
            event_id: mint_id(),
            answer: "a".to_owned(),
        };
        queued_at.insert(job.event_id.clone(), started);
        poster.deliver(&job).await;
    }
    let drained = Drained {
        started,
        ended: window_end(&poster.seen(), started),
        settled: poster.settled(),
        transactions: 0,
    };
    let mut report = Report::new(Lane::Outbound, Profile::Rig, Provenance::for_test());

    record(&mut report, &drained, &poster, &queued_at, &behaviours, 2)?;

    assert_eq!(report.measurements.get("delivered").copied(), Some(2.0));
    assert!(
        report.measurements.contains_key("others_p95_ms"),
        "both jobs were timed, so the head-of-line p95 is reported: {:?}",
        report.measurements
    );
    assert!(
        !report.measurements.contains_key("slow_p95_ms"),
        "no destination was scripted slow"
    );
    Ok(())
}
