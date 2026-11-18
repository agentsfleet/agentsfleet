//! Precedence diagnostics name inputs and never render their values.

use af_core::{env::MapEnv, test_util::trace::Capture};
use std::io::Write as _;

use super::{ConfigBuilder, ConfigOverrides};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn file_and_invocation_overrides_report_the_winner_without_values() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "[server]\nport = 3001")?;
    let environment = MapEnv::from_pairs([("PORT", "private-env-value")]);
    for (port, winner) in [(None, "configuration_file"), (Some(3002), "command_line")] {
        let config = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .overrides(ConfigOverrides { port })
            .build()?;
        let capture = Capture::install();
        config.announce();
        let event = capture.only("daemon_configuration_overrides_environment");
        assert_eq!(event.field("knob"), Some("PORT"));
        assert_eq!(event.field("source"), Some(winner));
        assert!(!format!("{event:?}").contains("private-env-value"));
    }
    Ok(())
}

#[test]
fn typed_pool_and_export_settings_report_ignored_environment_names_once() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(
        file,
        "[database.api]\nmax_connections = 3\n[database.migrator]\nmax_connections = 1\n\
        [telemetry]\notlp_endpoint = 'http://collector.example.com:4318'\n\
        [telemetry.otlp]\nexport_timeout_seconds = 3"
    )?;
    let environment = MapEnv::from_pairs([
        ("DATABASE_POOL_SIZE", "private-env-value"),
        ("DATABASE_POOL_SIZE_API", "private-env-value"),
        ("OTEL_EXPORTER_OTLP_TIMEOUT", "private-env-value"),
    ]);
    let config = ConfigBuilder::new(&environment)
        .file(Some(file.path()))
        .build()?;
    let refusal = config
        .preflight()
        .err()
        .ok_or("required serving secrets are absent")?;
    assert!(!refusal.knobs().contains(&"OTEL_EXPORTER_OTLP_TIMEOUT"));
    let capture = Capture::install();
    config.announce();
    let events = capture.events();
    let mut names: Vec<_> = events
        .iter()
        .filter_map(|event| event.field("knob"))
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "DATABASE_POOL_SIZE",
            "DATABASE_POOL_SIZE_API",
            "OTEL_EXPORTER_OTLP_TIMEOUT"
        ]
    );
    assert!(!format!("{events:?}").contains("private-env-value"));
    Ok(())
}
