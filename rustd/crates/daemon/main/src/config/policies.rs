//! Resolve component-owned public policies before any service is opened.

use super::{invalid, settings::ConfigToml};
use crate::preflight::runtime::OperationalSettings;

pub(super) fn read(
    settings: &garde::Valid<ConfigToml>,
) -> Result<OperationalSettings, crate::preflight::Refusal> {
    let tasks = garde::Unvalidated::new(settings.fleet.tasks)
        .validate()
        .map_err(|report| invalid(format!("fleet.tasks: {report}")))?;
    let authoring = garde::Unvalidated::new(settings.authoring.turn)
        .validate()
        .map_err(|report| invalid(format!("authoring.turn: {report}")))?;
    Ok(OperationalSettings {
        outbound_retries: afd_outbound::retry::RetryPolicy::new(
            &garde::Unvalidated::new(settings.outbound.retries)
                .validate()
                .map_err(|report| invalid(format!("outbound.retries: {report}")))?,
        ),
        inference: af_inference::Timeouts::new(
            &garde::Unvalidated::new(settings.inference)
                .validate()
                .map_err(|report| invalid(format!("inference: {report}")))?,
        ),
        tasks: afd_fleet::task::policy::TaskPolicy::new(&tasks),
        authoring: afd_authoring::turn_policy::TurnPolicy::new(&authoring),
        maintenance: super::maintenance::MaintenancePolicy::resolve(settings.maintenance)
            .map_err(|report| invalid(format!("maintenance: {report}")))?,
        telemetry: garde::Unvalidated::new(settings.telemetry.otlp)
            .validate()
            .map_err(|report| invalid(format!("telemetry.otlp: {report}")))?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::ConfigBuilder;
    use af_core::env::MapEnv;
    use std::io::Write as _;

    type Checked = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn inference_timeouts_reach_the_effective_policy() -> Checked {
        let mut file = tempfile::NamedTempFile::new()?;
        write!(
            file,
            "[inference]\nconnect_timeout_seconds=2\nstream_idle_timeout_seconds=7"
        )?;
        let environment = MapEnv::default();
        let config = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build()?;
        assert_eq!(config.runtime.operations.inference.connect().as_secs(), 2);
        assert_eq!(
            config.runtime.operations.inference.stream_idle().as_secs(),
            7
        );
        Ok(())
    }

    #[test]
    fn authoring_file_values_reach_the_effective_policy() -> Checked {
        let mut file = tempfile::NamedTempFile::new()?;
        write!(
            file,
            "[authoring.turn]\nmax_model_calls=1\nmax_wall_seconds=60\n\
            max_proposal_corrections=0\nprovider_timeout_seconds=30"
        )?;
        let environment = MapEnv::default();
        let config = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build()?;
        let settings = config.runtime.operations.authoring.settings();
        assert_eq!(settings.max_model_calls, 1);
        assert_eq!(settings.max_wall_seconds, 60);
        assert_eq!(settings.max_proposal_corrections, 0);
        assert_eq!(settings.provider_timeout_seconds, 30);
        Ok(())
    }

    #[test]
    fn authoring_policy_rejects_invalid_calls_corrections_and_deadlines() -> Checked {
        for setting in [
            "max_model_calls=0",
            "max_model_calls=9",
            "max_wall_seconds=0",
            "max_wall_seconds=301",
            "max_proposal_corrections=2",
            "provider_timeout_seconds=0",
            "provider_timeout_seconds=301",
        ] {
            let mut file = tempfile::NamedTempFile::new()?;
            write!(file, "[authoring.turn]\n{setting}")?;
            let environment = MapEnv::default();
            let failure = ConfigBuilder::new(&environment)
                .file(Some(file.path()))
                .build()
                .err()
                .ok_or("invalid authoring policy must refuse startup")?;
            assert!(failure.to_string().contains("authoring.turn"));
        }
        Ok(())
    }
}
