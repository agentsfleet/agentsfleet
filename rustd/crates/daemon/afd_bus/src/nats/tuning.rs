//! Every NATS adapter limit in one deserialisable value, so a configuration
//! file can set any of them; an absent field keeps its default.

use std::time::Duration;

use garde::{Unvalidated, Valid, Validate};
use serde::Deserialize;

use crate::{Result, error::ErrorKind};

/// Adapter limits, proven by [`Tuning::prove`] before any connection uses them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub struct Tuning {
    /// Messages a slow local reader may fall behind before it is told how
    /// many it lost.
    #[garde(range(min = 1, max = 65_536))]
    pub channel_capacity: usize,
    /// Messages the client buffers per native subscription before it reports
    /// a slow consumer. The pump drains each at once, so only a stalled
    /// runtime fills it.
    #[garde(range(min = 1, max = 1_048_576))]
    pub subscription_capacity: usize,
    /// A seed that does not answer inside this window refuses boot.
    #[garde(range(min = 100, max = 60_000))]
    pub connect_timeout_ms: u64,
    /// A publish or flush that cannot reach the client inside this window
    /// fails and is owed a gap, so an outage costs a caller this, not a hang.
    #[garde(range(min = 10, max = 10_000))]
    pub publish_deadline_ms: u64,
    /// Subjects published this recently before a disconnect are owed a marker.
    #[garde(range(min = 100, max = 60_000))]
    pub debt_window_ms: u64,
    /// Subjects the debt names before one broadcast marker replaces them.
    #[garde(range(min = 1, max = 65_536))]
    pub debt_subjects_max: usize,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            channel_capacity: 256,
            subscription_capacity: 4_096,
            connect_timeout_ms: 5_000,
            publish_deadline_ms: 250,
            debt_window_ms: 2_000,
            debt_subjects_max: 4_096,
        }
    }
}

impl Tuning {
    /// Check every bound once, at the boundary.
    ///
    /// # Errors
    /// `UZ-STARTUP-001` naming the field outside its range.
    pub fn prove(self) -> Result<Valid<Self>> {
        Unvalidated::new(self)
            .validate()
            .map_err(|source| ErrorKind::NatsTuning { source }.into())
    }

    /// The connect deadline.
    #[must_use]
    pub const fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.connect_timeout_ms)
    }

    /// The publish and flush deadline.
    #[must_use]
    pub const fn publish_deadline(&self) -> Duration {
        Duration::from_millis(self.publish_deadline_ms)
    }

    /// The window before a disconnect whose subjects are owed a marker.
    #[must_use]
    pub const fn debt_window(&self) -> Duration {
        Duration::from_millis(self.debt_window_ms)
    }
}

#[cfg(test)]
mod tests {
    use af_core::error_code;

    use super::Tuning;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    #[test]
    fn the_defaults_are_within_their_bounds() -> TestResult {
        let tuning = Tuning::default().prove()?;
        assert_eq!(tuning.connect_timeout().as_millis(), 5_000);
        assert_eq!(tuning.publish_deadline().as_millis(), 250);
        assert_eq!(tuning.debt_window().as_millis(), 2_000);
        Ok(())
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() -> TestResult {
        let tuning: Tuning = serde_json::from_str(r#"{"channel_capacity": 512}"#)?;
        assert_eq!(tuning.channel_capacity, 512);
        assert_eq!(
            Tuning {
                channel_capacity: 256,
                ..tuning
            },
            Tuning::default()
        );
        Ok(())
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let parsed = serde_json::from_str::<Tuning>(r#"{"channel_capactiy": 512}"#);
        assert!(parsed.is_err(), "a misspelt limit must not be ignored");
    }

    #[test]
    fn a_limit_outside_its_range_is_refused() -> TestResult {
        for tuning in [
            Tuning {
                channel_capacity: 0,
                ..Tuning::default()
            },
            Tuning {
                publish_deadline_ms: 0,
                ..Tuning::default()
            },
            Tuning {
                debt_subjects_max: 70_000,
                ..Tuning::default()
            },
        ] {
            let error = tuning
                .prove()
                .err()
                .ok_or("an out-of-range limit must refuse")?;
            assert_eq!(error.code(), error_code::STARTUP_ENV_CHECK);
            assert_eq!(error.reason(), "configuration");
        }
        Ok(())
    }
}
