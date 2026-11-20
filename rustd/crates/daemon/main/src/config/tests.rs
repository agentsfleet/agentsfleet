//! Exercise file presence and CLI precedence without mutating process state.

use std::io::Write as _;

use af_core::env::{EnvSource as _, MapEnv};

use super::{ConfigBuilder, ConfigOverrides, FILE_KNOB};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn file_settings_override_environment_and_keep_secret_lookup_injected() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    write!(
        file,
        "[server]\nport = 3100\n[runner.recovery]\nresume_from_checkpoint = false\n"
    )?;
    let environment = MapEnv::from_pairs([
        ("PORT", "3200"),
        ("DATABASE_URL_API", "injected-test-database"),
        ("AGENTSFLEET_RUNNER_DATA_MEMORY_MIB", "64"),
    ]);
    let source = ConfigBuilder::new(&environment)
        .file(Some(file.path()))
        .build()?;
    assert_eq!(source.environment().get("PORT").as_deref(), Some("3100"));
    assert_eq!(
        source.environment().get("DATABASE_URL_API"),
        environment.get("DATABASE_URL_API")
    );
    assert_eq!(
        source
            .environment()
            .get("AGENTSFLEET_RESUME_FROM_CHECKPOINT")
            .as_deref(),
        Some("false")
    );
    assert_eq!(
        source
            .environment()
            .get("AGENTSFLEET_RUNNER_DATA_MEMORY_MIB")
            .as_deref(),
        Some("64")
    );
    assert_eq!(source.environment().get(FILE_KNOB), None);
    Ok(())
}

#[test]
fn explicit_port_wins_and_preflight_does_not_reopen_the_file() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(file, "[server]\nport = 3100")?;
    let environment = MapEnv::from_pairs([(FILE_KNOB, file.path().to_str().ok_or("UTF-8 path")?)]);
    let source = ConfigBuilder::new(&environment)
        .overrides(ConfigOverrides { port: Some(3300) })
        .build()?;
    file.close()?;
    assert_eq!(source.environment().get("PORT").as_deref(), Some("3300"));
    let refusal = source
        .preflight()
        .err()
        .ok_or("serving still requires its secrets")?;
    assert!(refusal.knobs().contains(&"DATABASE_URL_API"));
    assert!(!refusal.knobs().contains(&FILE_KNOB));
    Ok(())
}

#[test]
fn invocation_overrides_merge_before_the_file_is_typed() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    writeln!(
        file,
        "[server]\nport = 'overridden-value'\nmax_sse_streams = 123"
    )?;
    let environment = MapEnv::default();
    let source = ConfigBuilder::new(&environment)
        .file(Some(file.path()))
        .overrides(ConfigOverrides { port: Some(3300) })
        .build()?;
    assert_eq!(source.environment().get("PORT").as_deref(), Some("3300"));
    assert_eq!(
        source.environment().get("SSE_MAX_STREAMS").as_deref(),
        Some("123")
    );
    Ok(())
}

#[test]
fn absent_file_preserves_environment_values() -> TestResult {
    let environment = MapEnv::from_pairs([("PORT", "3200"), (FILE_KNOB, "  ")]);
    let source = ConfigBuilder::new(&environment).build()?;
    assert_eq!(source.environment().get("PORT").as_deref(), Some("3200"));
    assert_eq!(source.environment().get("UNSET"), None);
    Ok(())
}

#[test]
fn malformed_unknown_and_unsafe_settings_refuse_without_echoing_values() -> TestResult {
    for contents in [
        "[server]\nport = 0",
        "[server]\napi_url = 'https://private-invalid-value@api.example.com'",
        "[telemetry]\notlp_endpoint = 'private-invalid-value'",
        "[server]\nport = 'private-invalid-value'",
        "[server]\nunknown = 'private-invalid-value'",
        "[runner.run]\nttl_seconds = 32",
        "[runner.scheduling]\nmax_concurrent_recovery_assignments = 0",
        "[runner.stream.data]\nmax_buffered_bytes = 1",
        "[runner.listener]\nmax_udp_payload_bytes = 1199",
        "[telemetry]\nlog_level = 'private-invalid-value'",
        "[server",
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(contents.as_bytes())?;
        let environment = MapEnv::default();
        let result = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build();
        let failure = result.err().ok_or("invalid settings must refuse startup")?;
        assert!(failure.to_string().contains(FILE_KNOB));
        assert!(!failure.to_string().contains("private-invalid-value"));
    }
    Ok(())
}

#[test]
fn explicit_unreadable_file_fails_even_when_environment_has_values() {
    let environment = MapEnv::from_pairs([("PORT", "3200")]);
    let result = ConfigBuilder::new(&environment)
        .file(Some(std::path::Path::new(
            "/missing-config-directory/agentsfleetd.toml",
        )))
        .build();
    assert!(result.is_err_and(|failure| failure.knobs() == [FILE_KNOB]));
}

#[test]
fn every_supported_file_section_reuses_its_existing_startup_input() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    let bucket_url = format!("https://{}.r2.cloudflarestorage.com", "a".repeat(32));
    let contents = COMPLETE_SETTINGS.replace("{endpoint}", &bucket_url);
    file.write_all(contents.as_bytes())?;
    let environment = MapEnv::from_pairs([("ENCRYPTION_MASTER_KEY", "private-env-value")]);
    let builder = ConfigBuilder::new(&environment).file(Some(file.path()));
    assert!(!format!("{builder:?}").contains("private-env-value"));
    let source = builder.build()?;
    assert_eq!(
        source
            .environment()
            .get("AGENTSFLEET_RUN_TTL_SECONDS")
            .as_deref(),
        Some("40")
    );
    assert_eq!(source.api_pool.max_connections, 8);
    assert_eq!(
        source.runtime.operations.telemetry.export_timeout_seconds,
        Some(3)
    );
    let storage: serde_json::Value = serde_json::from_str(
        &source
            .environment()
            .get("R2_CHECKPOINT_STORAGE")
            .ok_or("checkpoint location")?,
    )?;
    assert_eq!(
        storage
            .get("credential_name")
            .and_then(serde_json::Value::as_str),
        Some("checkpoint-fixture")
    );
    assert!(!format!("{source:?}").contains("private-env-value"));
    Ok(())
}

#[test]
fn every_documented_log_level_reaches_bootstrap_without_fallback() -> TestResult {
    for level in ["off", "error", "warn", "info", "debug", "trace"] {
        let mut file = tempfile::NamedTempFile::new()?;
        writeln!(file, "[telemetry]\nlog_level = '{level}'")?;
        let environment = MapEnv::default();
        let source = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build()?;
        assert_eq!(
            source.environment().get("AGENTSFLEET_LOG_LEVEL").as_deref(),
            Some(level)
        );
    }
    Ok(())
}

const COMPLETE_SETTINGS: &str = r#"
[server]
port = 3001
api_url = "https://api.example.com"
app_url = "https://app.example.com"
max_in_flight_requests = 128
max_sse_streams = 64
[database.api]
max_connections = 8
min_connections = 1
acquire_timeout_ms = 4000
[runner.listener]
address = "[::]:4443"
tls_cert_file = "/certs/server.pem"
tls_key_file = "/certs/server.key"
[runner.stream.control]
max_buffered_bytes = 16_777_216
[runner.stream.data]
max_buffered_bytes = 67_108_864
[runner.run]
ttl_seconds = 40
[runner.recovery]
resume_from_checkpoint = false

[runner.scheduling]
missed_wake_probe_seconds = 2
max_concurrent_recovery_assignments = 4
[nats]
dev_namespace = "dev-fixture"
wake_queue_capacity_fleets = 128
[identity]
issuer = "https://identity.example.com"
audience = "https://api.example.com"
jwks_url = "https://identity.example.com/keys"
clerk_api_base = "https://api.clerk.com/v1"
platform_admin_workspace_id = "01890a5d-ac96-774b-bcce-b302099a8062"
[connectors]
slack_api_url = "https://slack.com/api"
[telemetry]
log_level = "debug"
posthog_host = "https://events.example.com"
otlp_endpoint = "http://collector.example.com:4318"
otlp_protocol = "http/protobuf"
[telemetry.otlp]
export_timeout_seconds = 3
[checkpoint.storage]
endpoint = "{endpoint}"
bucket = "fixture-checkpoints"
credential_name = "checkpoint-fixture"
"#;

#[test]
fn fly_files_keep_the_exact_message_buffer_bytes() -> TestResult {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../deploy/fly");
    for deployment in ["agentsfleetd-dev", "agentsfleetd-prod"] {
        let path = base.join(deployment).join("agentsfleetd.toml");
        let environment = MapEnv::from_pairs([]);
        let source = ConfigBuilder::new(&environment).file(Some(&path)).build()?;
        let limits = af_stream::MemoryLimits::from_env(source.environment())?;
        let (incoming, outgoing) = af_stream::Budget::duplex(limits);
        assert_eq!(
            incoming.available(af_wire::protocol::StreamKind::Control),
            8_388_608
        );
        assert_eq!(
            outgoing.available(af_wire::protocol::StreamKind::Work),
            25_165_824
        );
        assert_eq!(
            source
                .environment()
                .get("AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY")
                .as_deref(),
            Some("64")
        );
        assert_eq!(
            source.environment().get("AGENTSFLEET_LOG_LEVEL").as_deref(),
            Some("info")
        );
        assert_eq!(
            source
                .environment()
                .get(super::super::preflight::runner_listener::MAX_UDP_PAYLOAD_BYTES)
                .as_deref(),
            Some("1232")
        );
    }
    Ok(())
}

#[test]
fn a_file_pool_floor_must_fit_the_selected_ceiling() -> TestResult {
    let mut file = tempfile::NamedTempFile::new()?;
    write!(
        file,
        "[database.api]\nmax_connections = 3\nmin_connections = 4"
    )?;
    let environment = MapEnv::default();
    let failure = ConfigBuilder::new(&environment)
        .file(Some(file.path()))
        .build()
        .err()
        .ok_or("an impossible pool floor must refuse before startup")?;
    assert_eq!(failure.knobs(), ["database.api"]);
    assert!(failure.to_string().contains("min_connections"));
    Ok(())
}
