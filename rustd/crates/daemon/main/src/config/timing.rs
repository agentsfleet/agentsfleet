//! The daemon's file shape resolves into the shared, cross-checked lease clock.

use af_core::timing::LeaseSettings;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Health {
    #[serde(rename = "heartbeat_interval_seconds")]
    pub heartbeat_interval: u64,
    #[serde(rename = "offline_after_seconds")]
    pub offline_after: Option<u64>,
    #[serde(rename = "healthy_before_uncordon_seconds")]
    pub healthy_before_uncordon: u64,
}

impl Default for Health {
    fn default() -> Self {
        let defaults = LeaseSettings::default();
        Self {
            heartbeat_interval: defaults.heartbeat_interval_seconds,
            offline_after: defaults.offline_after_seconds,
            healthy_before_uncordon: defaults.healthy_before_uncordon_seconds,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Sandboxes {
    pub max_idle_hold_seconds: u64,
}

impl Default for Sandboxes {
    fn default() -> Self {
        Self {
            max_idle_hold_seconds: LeaseSettings::default().sandbox_idle_hold_seconds,
        }
    }
}

pub(super) fn settings(runner: &super::settings::Runner) -> LeaseSettings {
    let defaults = LeaseSettings::default();
    LeaseSettings {
        ttl_seconds: runner.lease.ttl_seconds.unwrap_or(defaults.ttl_seconds),
        max_runtime_seconds: runner
            .lease
            .max_runtime_seconds
            .unwrap_or(defaults.max_runtime_seconds),
        heartbeat_interval_seconds: runner.health.heartbeat_interval,
        offline_after_seconds: runner.health.offline_after,
        healthy_before_uncordon_seconds: runner.health.healthy_before_uncordon,
        sandbox_idle_hold_seconds: runner.sandboxes.max_idle_hold_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ConfigBuilder;
    use af_core::env::MapEnv;
    use std::{io::Write as _, time::Duration};

    type Checked = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn all_timing_sections_reach_the_checked_runtime_snapshot() -> Checked {
        let mut file = tempfile::NamedTempFile::new()?;
        write!(
            file,
            "[runner.lease]\nttl_seconds=40\nmax_runtime_seconds=3600\n\
            [runner.health]\nheartbeat_interval_seconds=7\noffline_after_seconds=90\n\
            healthy_before_uncordon_seconds=21\n[runner.sandboxes]\nmax_idle_hold_seconds=45"
        )?;
        let environment = MapEnv::from_pairs([("AGENTSFLEET_LEASE_TTL_SECONDS", "invalid")]);
        let config = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build()?;
        let clock = config.runtime.clock;
        assert_eq!(clock.ttl(), Duration::from_secs(40));
        assert_eq!(clock.max_runtime(), Duration::from_secs(3600));
        assert_eq!(clock.heartbeat_interval(), Duration::from_secs(7));
        assert_eq!(clock.offline_after(), Duration::from_secs(90));
        assert_eq!(clock.uncordon_after(), Duration::from_secs(21));
        assert_eq!(clock.sandbox_idle_hold(), Duration::from_secs(45));
        Ok(())
    }

    #[test]
    fn conflicting_health_runtime_and_hold_settings_refuse_startup() -> Checked {
        for contents in [
            "[runner.lease]\nmax_runtime_seconds=35",
            "[runner.lease]\nmax_runtime_seconds=43201",
            "[runner.health]\nheartbeat_interval_seconds=0",
            "[runner.health]\nheartbeat_interval_seconds=53\nhealthy_before_uncordon_seconds=60",
            "[runner.health]\noffline_after_seconds=34",
            "[runner.health]\nhealthy_before_uncordon_seconds=10",
            "[runner.health]\nhealthy_before_uncordon_seconds=105",
            "[runner.sandboxes]\nmax_idle_hold_seconds=0",
        ] {
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(contents.as_bytes())?;
            let environment = MapEnv::default();
            let failure = ConfigBuilder::new(&environment)
                .file(Some(file.path()))
                .build()
                .err()
                .ok_or("unsafe timing must refuse startup")?;
            assert!(failure.to_string().contains("runner"));
        }
        Ok(())
    }
}
