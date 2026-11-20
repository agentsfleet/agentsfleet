//! Replicas spread their first maintenance pass without extending its cadence.

use std::{collections::hash_map::RandomState, hash::BuildHasher as _, time::Duration};

#[derive(Debug, Clone)]
pub(super) struct Startup {
    seed: RandomState,
    spread: Duration,
}

impl Startup {
    pub(super) fn new(spread_ms: u64) -> Self {
        Self {
            seed: RandomState::new(),
            spread: Duration::from_millis(spread_ms),
        }
    }

    pub(super) fn delay(&self, name: &'static str, period: Duration) -> Duration {
        let bound = u64::try_from(self.spread.min(period).as_millis()).unwrap_or(u64::MAX);
        Duration::from_millis(
            self.seed
                .hash_one(name)
                .checked_rem(bound.saturating_add(1))
                .unwrap_or(0),
        )
    }

    pub(super) fn delayed_first_pass(&self, name: &'static str, period: Duration) -> Duration {
        period
            .saturating_sub(self.spread.min(period))
            .saturating_add(self.delay(name, period))
    }

    pub(super) fn interval(&self, name: &'static str, period: Duration) -> tokio::time::Interval {
        let first = tokio::time::Instant::now() + self.delay(name, period);
        let mut interval = tokio::time::interval_at(first, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, Startup};

    #[test]
    fn delayed_jobs_keep_their_first_pass_near_the_original_cadence() {
        let startup = Startup::new(5000);
        for name in ["liveness", "census", "billing"] {
            let first = startup.delayed_first_pass(name, Duration::from_secs(3600));
            assert!(first >= Duration::from_secs(3595));
            assert!(first <= Duration::from_secs(3600));
            assert!(startup.delay(name, Duration::from_secs(2)) <= Duration::from_secs(2));
        }
        let fixed = Startup::new(0);
        assert_eq!(
            fixed.delayed_first_pass("census", Duration::from_secs(3600)),
            Duration::from_secs(3600)
        );
        assert_eq!(
            fixed.delay("billing", Duration::from_secs(5)),
            Duration::ZERO
        );
    }
}
