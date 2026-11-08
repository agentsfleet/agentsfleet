//! The brake's windows, on an explicit clock so no test waits.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use af_core::id::Uuid7;
use af_fleet_runtime::config::{AnomalyRule, Pattern};

use super::{Anomaly, Brake, Key, judge};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const FLEET: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";

fn rule(repeats: u32, window_s: u32) -> Result<AnomalyRule, &'static str> {
    Ok(AnomalyRule {
        pattern: Pattern::SameAction,
        repeats: NonZeroU32::new(repeats).ok_or("repeats is nonzero")?,
        window_s: NonZeroU32::new(window_s).ok_or("window is nonzero")?,
    })
}

fn key(action: &str, window_s: u32) -> Result<Key, Box<dyn std::error::Error + Send + Sync>> {
    Ok(Key {
        fleet: Uuid7::parse(FLEET)?,
        tool: "shell".to_owned(),
        action: action.to_owned(),
        window_s,
    })
}

#[test]
fn the_two_outcomes_are_distinct() {
    assert_ne!(Anomaly::Normal, Anomaly::AutoKill);
}

#[test]
fn each_rule_counts_its_own_window() -> TestResult {
    let brake = Brake::new(64);
    let fleet = Uuid7::parse(FLEET)?;
    let rules = [rule(3, 60)?, rule(10, 3_600)?];
    let start = Instant::now();
    for _ in 0..2 {
        assert_eq!(judge(&brake, &fleet, "shell", "run", &rules, start), None);
    }
    assert_eq!(
        judge(&brake, &fleet, "shell", "run", &rules, start),
        Some((3, 3)),
        "the third call inside 60 s trips the first rule"
    );
    let later = start + Duration::from_secs(61);
    assert_eq!(judge(&brake, &fleet, "shell", "run", &rules, later), None);
    assert_eq!(
        brake.hit(key("run", 60)?, later),
        2,
        "the first window restarted at 1"
    );
    assert_eq!(
        brake.hit(key("run", 3_600)?, later),
        5,
        "the hour window kept every call, the tripping one included"
    );
    Ok(())
}

#[test]
fn a_single_shared_key_would_fail_this() -> TestResult {
    // The retired counter used one key for every rule. Under it the hour rule
    // below would read the minute rule's three calls as its own and trip early.
    let brake = Brake::new(64);
    let fleet = Uuid7::parse(FLEET)?;
    let rules = [rule(100, 60)?, rule(3, 3_600)?];
    let now = Instant::now();
    assert_eq!(judge(&brake, &fleet, "shell", "run", &rules, now), None);
    assert_eq!(judge(&brake, &fleet, "shell", "run", &rules, now), None);
    assert_eq!(
        judge(&brake, &fleet, "shell", "run", &rules, now),
        Some((3, 3))
    );
    Ok(())
}

#[test]
fn anomaly_keys_stay_bounded() -> TestResult {
    const MAX: usize = 16;
    let brake = Brake::new(MAX);
    let start = Instant::now();
    for index in 0..=MAX {
        let at = start + Duration::from_millis(u64::try_from(index)?);
        brake.hit(key(&format!("action-{index}"), 60)?, at);
    }
    assert_eq!(brake.len(), MAX, "the cap holds");
    assert_eq!(
        brake.hit(key("action-0", 60)?, start + Duration::from_secs(1)),
        1,
        "the oldest window was the one evicted"
    );
    assert_eq!(
        brake.hit(
            key(&format!("action-{MAX}"), 60)?,
            start + Duration::from_secs(1)
        ),
        2,
        "the newest window survived"
    );
    Ok(())
}
