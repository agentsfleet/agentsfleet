//! Timers and queues refuse unsafe settings while preserving partial-file defaults.

use std::io::Write as _;

use af_core::env::MapEnv;

use super::{ConfigBuilder, FILE_KNOB};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn timer_and_queue_bounds_refuse_before_services_open() -> TestResult {
    let environment = MapEnv::default();
    for (section, setting) in [
        ("maintenance", "startup_spread_ms = 5001"),
        ("maintenance.runner_liveness", "batch_size = 0"),
        ("maintenance.runner_liveness", "interval_seconds = 0"),
        ("maintenance.runner_history", "keep_days = 0"),
        (
            "maintenance.runner_history",
            "idle_interval_seconds = 5\nbacklog_interval_seconds = 6",
        ),
        ("maintenance.runner_history", "max_batches = 0"),
        (
            "maintenance.billing",
            "interval_seconds = 10\nretry_oldest_after_seconds = 9",
        ),
        ("maintenance.billing", "max_drain_passes = 0"),
        ("maintenance.durable_tasks", "pass_timeout_seconds = 0"),
        ("maintenance.outbound_delivery", "interval_seconds = 0"),
        (
            "telemetry.otlp",
            "trace_queue_capacity = 512\ntrace_batch_size = 513",
        ),
        (
            "telemetry.otlp",
            "log_queue_capacity = 512\nlog_batch_size = 513",
        ),
        ("telemetry.otlp", "metrics_export_interval_seconds = 0"),
        ("telemetry.otlp", "export_timeout_seconds = 0"),
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        writeln!(file, "[{section}]\n{setting}")?;
        let failure = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build()
            .err()
            .ok_or("an unsafe timer or queue must refuse")?;
        assert_eq!(failure.knobs(), [FILE_KNOB]);
        assert!(
            failure
                .to_string()
                .contains(section.split('.').next().unwrap_or(section))
        );
    }
    Ok(())
}

#[test]
fn partial_maintenance_sections_keep_their_own_defaults() -> TestResult {
    let environment = MapEnv::default();
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(
        file,
        "[maintenance.checkpoint_storage]\ninterval_seconds = 20"
    )?;
    let source = ConfigBuilder::new(&environment)
        .file(Some(file.path()))
        .build()?;
    let limits = source.runtime.operations.maintenance;
    assert_eq!(limits.loops.checkpoint_storage.interval_seconds, 20);
    assert_eq!(limits.loops.checkpoint_storage.pass_timeout_seconds, 240);
    assert_eq!(limits.loops.durable_tasks.interval_seconds, 10);
    assert_eq!(limits.loops.durable_tasks.pass_timeout_seconds, 30);
    assert_eq!(limits.loops.billing.retry_oldest_after_seconds, 60);
    assert_eq!(limits.loops.startup_spread_ms, 5000);
    assert_eq!(limits.liveness.batch_size, 100);
    assert_eq!(limits.retention.keep_days, 30);
    Ok(())
}
