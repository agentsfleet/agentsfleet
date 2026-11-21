//! Resolve component-owned public policies before any service is opened.

use super::{invalid, settings::ConfigToml};
use crate::preflight::runtime::OperationalSettings;

pub(super) fn read(
    settings: &garde::Valid<ConfigToml>,
) -> Result<OperationalSettings, crate::preflight::Refusal> {
    let tasks = garde::Unvalidated::new(settings.fleet.tasks)
        .validate()
        .map_err(|report| invalid(format!("fleet.tasks: {report}")))?;
    Ok(OperationalSettings {
        outbound_retries: afd_outbound::retry::RetryPolicy::new(
            &garde::Unvalidated::new(settings.outbound.retries)
                .validate()
                .map_err(|report| invalid(format!("outbound.retries: {report}")))?,
        ),
        tasks: afd_fleet::task::policy::TaskPolicy::new(&tasks),
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
}
