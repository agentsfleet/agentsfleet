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
    insert(
        values,
        "AGENTSFLEET_DEV_NAMESPACE",
        settings.nats.dev_namespace.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_WAKE_QUEUE_CAPACITY",
        settings.nats.wake_queue_capacity_fleets,
    );
}

fn runner(values: &mut Values, settings: &ConfigToml) {
    let runner = &settings.runner;
    listener(values, &runner.listener);
    insert(
        values,
        af_stream::CONTROL_BUFFER_BYTES_ENV,
        runner.stream.control.max_buffered_bytes,
    );
    insert(
        values,
        af_stream::DATA_BUFFER_BYTES_ENV,
        runner.stream.data.max_buffered_bytes,
    );
    insert(
        values,
        "AGENTSFLEET_RUN_TTL_SECONDS",
        runner.run.ttl_seconds,
    );
    insert(
        values,
        "AGENTSFLEET_RESUME_FROM_CHECKPOINT",
        runner.recovery.resume_from_checkpoint,
    );
    insert(
        values,
        "AGENTSFLEET_ASSIGN_FALLBACK_SECONDS",
        runner.scheduling.missed_wake_probe_seconds,
    );
    insert(
        values,
        "AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS",
        runner.scheduling.max_concurrent_recovery_claims,
    );
}

fn listener(values: &mut Values, listener: &super::settings::Listener) {
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_LISTEN_ADDR",
        listener.address.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_TLS_CERT_FILE",
        listener.tls_cert_file.as_ref(),
    );
    insert(
        values,
        "AGENTSFLEET_RUNNER_STREAM_TLS_KEY_FILE",
        listener.tls_key_file.as_ref(),
    );
    insert(
        values,
        crate::preflight::runner_listener::MAX_UDP_PAYLOAD_BYTES,
        listener.max_udp_payload_bytes,
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
}
