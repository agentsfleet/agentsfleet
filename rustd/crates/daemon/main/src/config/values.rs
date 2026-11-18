//! Preserve file presence while reusing each service's environment validator.

use std::collections::BTreeMap;

use super::settings::ConfigToml;

type Values = BTreeMap<&'static str, String>;

pub(super) fn collect(settings: &garde::Valid<ConfigToml>) -> Values {
    let mut values = Values::new();
    server(&mut values, settings);
    runner(&mut values, settings);
    identity(&mut values, settings);
    telemetry(&mut values, settings);
    if let Some(storage) = &settings.checkpoint.storage {
        let storage = serde_json::json!({
            "endpoint": storage.endpoint,
            "bucket": storage.bucket,
            "credential_name": storage.credential_name,
        });
        values.insert("R2_CHECKPOINT_STORAGE", storage.to_string());
    }
    values
}

fn insert<T: ToString>(values: &mut Values, key: &'static str, value: Option<T>) {
    if let Some(value) = value {
        values.insert(key, value.to_string());
    }
}

fn server(values: &mut Values, settings: &ConfigToml) {
    let server = &settings.server;
    insert(values, "PORT", server.port);
    insert(values, "API_URL", server.api_url.as_ref());
    insert(values, "APP_URL", server.app_url.as_ref());
    insert(
        values,
        "AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS",
        server.max_in_flight_requests,
    );
    insert(values, "SSE_MAX_STREAMS", server.max_sse_streams);
    let pool = &settings.database.api;
    insert(values, "DATABASE_POOL_SIZE_API", pool.max_connections);
    insert(values, "DATABASE_MIN_POOL_SIZE_API", pool.min_connections);
    insert(
        values,
        "DATABASE_ACQUIRE_TIMEOUT_MS_API",
        pool.acquire_timeout_ms,
    );
    insert(
        values,
        "AGENTSFLEET_DEV_NAMESPACE",
        settings.bus.dev_namespace.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY",
        settings.bus.wake_queue_capacity,
    );
}

fn runner(values: &mut Values, settings: &ConfigToml) {
    let runner = &settings.runner;
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR",
        runner.listener.address.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE",
        runner.listener.tls_cert_file.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE",
        runner.listener.tls_key_file.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_CONTROL_MEMORY_MIB",
        runner.stream.control_mib,
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_DATA_MEMORY_MIB",
        runner.stream.data_mib,
    );
    insert(
        values,
        "AGENTSFLEET_LEASE_TTL_SECONDS",
        runner.lease.ttl_seconds,
    );
    insert(
        values,
        "AGENTSFLEET_RESUME_FROM_CHECKPOINT",
        runner.recovery.resume_from_checkpoint,
    );
    insert(
        values,
        "AGENTSFLEET_OFFER_FALLBACK_SECONDS",
        runner.recovery.offer_fallback_seconds,
    );
    insert(
        values,
        "AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS",
        runner.recovery.max_concurrent_claims,
    );
}

fn identity(values: &mut Values, settings: &ConfigToml) {
    let identity = &settings.identity;
    insert(values, "OIDC_ISSUER", identity.issuer.as_ref());
    insert(values, "OIDC_AUDIENCE", identity.audience.as_ref());
    insert(values, "OIDC_JWKS_URL", identity.jwks_url.as_ref());
    insert(values, "CLERK_API_BASE", identity.clerk_api_base.as_ref());
    insert(
        values,
        "PLATFORM_ADMIN_WORKSPACE_ID",
        identity.platform_admin_workspace_id.as_ref(),
    );
    insert(
        values,
        "SLACK_API_URL",
        settings.connectors.slack_api_url.as_ref(),
    );
}

fn telemetry(values: &mut Values, settings: &ConfigToml) {
    let telemetry = &settings.telemetry;
    insert(
        values,
        "AGENTSFLEET_LOG_LEVEL",
        telemetry.log_level.as_ref(),
    );
    insert(values, "POSTHOG_HOST", telemetry.posthog_host.as_ref());
    insert(
        values,
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        telemetry.otlp_endpoint.as_ref(),
    );
    insert(
        values,
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        telemetry.otlp_protocol.as_ref(),
    );
    insert(
        values,
        "OTEL_EXPORTER_OTLP_TIMEOUT",
        telemetry.otlp_timeout_ms,
    );
}
