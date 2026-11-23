//! Resolve component-owned public policies before any service is opened.

use super::{invalid, settings::ConfigToml};
use crate::preflight::runtime::OperationalSettings;

pub(super) fn read(
    settings: &garde::Valid<ConfigToml>,
) -> Result<OperationalSettings, crate::preflight::Refusal> {
    let tasks = garde::Unvalidated::new(settings.fleet.durable_tasks)
        .validate()
        .map_err(|report| invalid(format!("fleet.durable_tasks: {report}")))?;
    Ok(OperationalSettings {
        outbound_retries: afd_outbound::retry::RetryPolicy::new(
            &garde::Unvalidated::new(settings.outbound.retries)
                .validate()
                .map_err(|report| invalid(format!("outbound.retries: {report}")))?,
        ),
        tasks: afd_fleet::durable_task::policy::DurableTaskPolicy::new(&tasks),
        telemetry: garde::Unvalidated::new(settings.telemetry.otlp)
            .validate()
            .map_err(|report| invalid(format!("telemetry.otlp: {report}")))?,
        tuning: super::tuning::Tuning::from(&**settings),
        scheduler_retries: settings.connectors.qstash.policy().ok_or_else(|| {
            invalid(
                "connectors.qstash: every attempt and wait must end inside call_budget_seconds, \
                 the budget under 30 seconds, and max_retry_delay_ms at least 200"
                    .to_owned(),
            )
        })?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::ConfigBuilder;
    use af_core::env::MapEnv;
    use std::io::Write as _;

    /// The setup runtime is a constant: a file still naming the retired
    /// `[inference]` or `[authoring.turn]` section is refused, not ignored.
    #[test]
    fn a_retired_setup_section_refuses_startup() -> Result<(), Box<dyn std::error::Error>> {
        for section in [
            "[inference]\nconnect_timeout_seconds=2",
            "[authoring.turn]\nmax_wall_seconds=60",
        ] {
            let mut file = tempfile::NamedTempFile::new()?;
            write!(file, "{section}")?;
            let environment = MapEnv::default();
            let refused = ConfigBuilder::new(&environment)
                .file(Some(file.path()))
                .build();
            assert!(refused.is_err(), "{section} was accepted");
        }
        Ok(())
    }

    /// The daemon runs no timed loop (Indy, Nov 21, 2026), so a file still
    /// tuning one is refused rather than silently ignored.
    #[test]
    fn a_timed_loop_setting_refuses_startup() -> Result<(), Box<dyn std::error::Error>> {
        for section in [
            "[maintenance]\nstartup_spread_ms = 5000",
            "[maintenance.runner_liveness]\ninterval_seconds = 10",
            "[maintenance.outbound_delivery]\ninterval_seconds = 30",
            "[maintenance.runner_history]\nkeep_days = 30",
            "[runner.scheduling]\nmissed_wake_probe_seconds = 2",
        ] {
            let mut file = tempfile::NamedTempFile::new()?;
            write!(file, "{section}")?;
            let environment = MapEnv::default();
            let refused = ConfigBuilder::new(&environment)
                .file(Some(file.path()))
                .build();
            assert!(refused.is_err(), "{section} was accepted");
        }
        Ok(())
    }

    /// Out-of-range retry and export settings refuse startup, naming their section.
    #[test]
    fn out_of_range_retry_and_export_settings_refuse_startup_by_section()
    -> Result<(), Box<dyn std::error::Error>> {
        for (contents, section) in [
            ("[outbound.retries]\nmax_attempts = 11", "outbound.retries"),
            ("[outbound.retries]\nmax_attempts = 0", "outbound.retries"),
            (
                "[telemetry.otlp]\nmetrics_export_interval_seconds = 0",
                "telemetry.otlp",
            ),
            (
                "[telemetry.otlp]\ntrace_queue_capacity = 8\ntrace_batch_size = 9",
                "telemetry.otlp",
            ),
        ] {
            let mut file = tempfile::NamedTempFile::new()?;
            write!(file, "{contents}")?;
            let environment = MapEnv::default();
            let refusal = ConfigBuilder::new(&environment)
                .file(Some(file.path()))
                .build()
                .err()
                .ok_or("an out-of-range setting was accepted")?;
            assert_eq!(refusal.knobs(), [super::super::FILE_KNOB], "{contents}");
            assert!(
                refusal.to_string().contains(&format!("{section}: ")),
                "{contents}: {refusal}"
            );
        }
        Ok(())
    }
}
