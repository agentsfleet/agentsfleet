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
    assert_eq!(source.get("PORT").as_deref(), Some("3100"));
    assert_eq!(
        source.get("DATABASE_URL_API"),
        environment.get("DATABASE_URL_API")
    );
    assert_eq!(
        source.get("AGENTSFLEET_RESUME_FROM_CHECKPOINT").as_deref(),
        Some("false")
    );
    assert_eq!(
        source.get("AGENTSFLEET_RUNNER_DATA_MEMORY_MIB").as_deref(),
        Some("64")
    );
    assert_eq!(source.get(FILE_KNOB), None);
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
    let nested = ConfigBuilder::new(&source).build()?;
    assert_eq!(nested.get("PORT").as_deref(), Some("3300"));
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
    assert_eq!(source.get("PORT").as_deref(), Some("3300"));
    assert_eq!(source.get("SSE_MAX_STREAMS").as_deref(), Some("123"));
    Ok(())
}

#[test]
fn absent_file_preserves_environment_values() -> TestResult {
    let environment = MapEnv::from_pairs([("PORT", "3200"), (FILE_KNOB, "  ")]);
    let source = ConfigBuilder::new(&environment).build()?;
    assert_eq!(source.get("PORT").as_deref(), Some("3200"));
    assert_eq!(source.get("UNSET"), None);
    Ok(())
}

#[test]
fn malformed_unknown_and_unsafe_settings_refuse_without_echoing_values() -> TestResult {
    for contents in [
        "[server]\nport = 0",
        "[server]\nport = 'private-invalid-value'",
        "[server]\nunknown = 'private-invalid-value'",
        "[runner.lease]\nttl_seconds = 32",
        "[runner.recovery]\nmax_concurrent_claims = 0",
        "[runner.stream]\ndata_mib = 1",
        "[server",
    ] {
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(contents.as_bytes())?;
        let environment = MapEnv::default();
        let result = ConfigBuilder::new(&environment)
            .file(Some(file.path()))
            .build();
        let failure = result.err().ok_or("invalid settings must refuse startup")?;
        assert!(failure.to_string().contains(super::FILE_KNOB));
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
    let endpoint = format!("https://{}.r2.cloudflarestorage.com", "a".repeat(32));
    let contents = COMPLETE_SETTINGS.replace("{endpoint}", &endpoint);
    file.write_all(contents.as_bytes())?;
    let environment = MapEnv::from_pairs([("ENCRYPTION_MASTER_KEY", "private-env-value")]);
    let builder = ConfigBuilder::new(&environment).file(Some(file.path()));
    assert!(!format!("{builder:?}").contains("private-env-value"));
    let source = builder.build()?;
    assert_eq!(
        source.get("AGENTSFLEET_LEASE_TTL_SECONDS").as_deref(),
        Some("40")
    );
    assert_eq!(source.get("DATABASE_POOL_SIZE_API").as_deref(), Some("8"));
    assert_eq!(
        source.get("OTEL_EXPORTER_OTLP_TIMEOUT").as_deref(),
        Some("3000")
    );
    let storage: serde_json::Value = serde_json::from_str(
        &source
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
[runner.stream]
control_mib = 16
data_mib = 64
[runner.lease]
ttl_seconds = 40
[runner.recovery]
resume_from_checkpoint = false
offer_fallback_seconds = 2
max_concurrent_claims = 4
[bus]
dev_namespace = "dev-fixture"
wake_queue_capacity = 128
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
otlp_timeout_ms = 3000
[checkpoint.storage]
endpoint = "{endpoint}"
bucket = "fixture-checkpoints"
credential_name = "checkpoint-fixture"
"#;
