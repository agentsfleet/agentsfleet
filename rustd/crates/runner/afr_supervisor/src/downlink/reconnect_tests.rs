use super::{HTTP3_SKIP, Outcome, Preference, Recovery};
use af_stream::Reason;
use std::{collections::HashSet, time::Duration};
use tokio::time::Instant;

type Proof<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn short_sessions_keep_backoff_until_a_stable_session_resets_it() -> crate::Result<()> {
    let mut recovery = Recovery::new(crate::client::backoff().with_jitter_seed(7));
    let first = recovery.after(None)?;
    assert!((Duration::from_millis(250)..Duration::from_millis(500)).contains(&first));
    let second = recovery.after(Some(Duration::from_secs(1)))?;
    assert!((Duration::from_millis(500)..Duration::from_secs(1)).contains(&second));
    let third = recovery.after(Some(Duration::from_secs(59)))?;
    assert!((Duration::from_secs(1)..Duration::from_secs(2)).contains(&third));
    assert_eq!(recovery.after(Some(Duration::from_secs(60)))?, first);
    Ok(())
}

#[test]
fn reconnects_spread_over_the_jitter_window_and_never_exceed_the_cap() -> crate::Result<()> {
    let mut buckets = HashSet::new();
    for seed in 0..100 {
        let mut recovery = Recovery::new(crate::client::backoff().with_jitter_seed(seed));
        let first = recovery.after(None)?;
        assert!(first <= Duration::from_secs(2));
        buckets.insert(first.as_millis() / 50);
        for _ in 0..20 {
            assert!(recovery.after(Some(Duration::from_secs(1)))? <= Duration::from_secs(30));
        }
    }
    assert!(
        buckets.len() >= 3,
        "100 reconnects must occupy several deterministic time buckets"
    );
    Ok(())
}

/// The window as the runner's operators know it, written out so a change to the constant fails.
#[test]
fn the_http3_skip_window_is_thirty_seconds() {
    assert_eq!(HTTP3_SKIP, Duration::from_secs(30));
}

#[test]
fn http3_failing_while_http2_carries_skips_http3_for_exactly_the_window() {
    let start = Instant::now();
    let mut preference = Preference::default();
    preference.record(
        Outcome::Carried {
            http3_failure: Some(Reason::Timeout),
        },
        start,
    );
    assert!(preference.skips_http3(start));
    assert!(preference.skips_http3(start + HTTP3_SKIP - Duration::from_millis(1)));
    assert!(!preference.skips_http3(start + HTTP3_SKIP));
}

#[test]
fn a_connect_over_http3_never_skips_http3() {
    let start = Instant::now();
    let mut preference = Preference::default();
    preference.record(
        Outcome::Carried {
            http3_failure: None,
        },
        start,
    );
    assert!(!preference.skips_http3(start));
}

#[test]
fn neither_carrier_connecting_leaves_http3_available_to_the_next_attempt() {
    let start = Instant::now();
    let mut preference = Preference::default();
    preference.record(Outcome::Failed, start);
    assert!(
        !preference.skips_http3(start),
        "a failed attempt sent the next one straight to HTTP/2"
    );
}

#[derive(Clone, Copy, Debug)]
enum Step {
    /// HTTP/3 failed and HTTP/2 carried: the one outcome that suggests UDP is the problem.
    UdpSuspect,
    /// HTTP/3 carried.
    Clean,
    /// Neither carrier connected.
    Failed,
}

impl Step {
    fn outcome(self) -> Outcome {
        match self {
            Self::UdpSuspect => Outcome::Carried {
                http3_failure: Some(Reason::Timeout),
            },
            Self::Clean => Outcome::Carried {
                http3_failure: None,
            },
            Self::Failed => Outcome::Failed,
        }
    }
}

/// Every history of up to `depth` steps, the empty one included.
fn histories(depth: usize) -> Vec<Vec<Step>> {
    let mut all = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..depth {
        frontier = frontier
            .iter()
            .flat_map(|history| {
                [Step::UdpSuspect, Step::Clean, Step::Failed]
                    .into_iter()
                    .map(move |step| {
                        let mut longer = history.clone();
                        longer.push(step);
                        longer
                    })
            })
            .collect();
        all.extend(frontier.iter().cloned());
    }
    all
}

/// The independent oracle: HTTP/3 is skipped at `now` exactly when the most recent `UdpSuspect`
/// step happened less than the window ago. It reads no production logic, only the steps and the
/// window constant, and it restates the rule on purpose so a wrong rule cannot agree with itself.
///
/// Steps are a full window apart because `connect` records nothing while the skip is active, so
/// no caller records inside one.
#[test]
fn http3_is_skipped_exactly_while_the_latest_udp_suspicion_is_fresh_after_any_history() -> Proof {
    let origin = Instant::now();
    let histories = histories(6);
    assert_eq!(histories.len(), 1_093, "3^0 + 3^1 + ... + 3^6 histories");
    let after_the_last_step = [
        Duration::ZERO,
        Duration::from_secs(10),
        HTTP3_SKIP.saturating_sub(Duration::from_millis(1)),
        HTTP3_SKIP,
        HTTP3_SKIP + Duration::from_millis(1),
        HTTP3_SKIP * 2,
    ];
    let mut checked = 0_usize;
    for history in &histories {
        let mut preference = Preference::default();
        let mut suspected_at: Option<Instant> = None;
        let mut last_step_at = origin;
        for (index, step) in history.iter().enumerate() {
            last_step_at = origin + HTTP3_SKIP.saturating_mul(u32::try_from(index)?);
            preference.record(step.outcome(), last_step_at);
            if matches!(step, Step::UdpSuspect) {
                suspected_at = Some(last_step_at);
            }
        }
        for after in after_the_last_step {
            let now = last_step_at + after;
            let expected = suspected_at.is_some_and(|suspected| now < suspected + HTTP3_SKIP);
            assert_eq!(
                preference.skips_http3(now),
                expected,
                "after {history:?}, probing {after:?} past the last step"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, histories.len() * after_the_last_step.len());
    Ok(())
}
