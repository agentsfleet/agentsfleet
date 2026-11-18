//! Only deployment inputs accepted in the application file.

use garde::Validate;
use serde::Deserialize;

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ConfigToml {
    #[garde(skip)]
    pub maintenance: super::maintenance::Maintenance,
    #[garde(skip)]
    pub fleet: Fleet,
    #[garde(skip)]
    pub authoring: Authoring,
    #[garde(dive)]
    pub server: Server,
    #[garde(dive)]
    pub database: Database,
    #[garde(dive)]
    pub runner: Runner,
    #[garde(dive)]
    pub nats: Nats,
    #[garde(dive)]
    pub identity: Identity,
    #[garde(dive)]
    pub connectors: Connectors,
    #[garde(dive)]
    pub telemetry: Telemetry,
    #[garde(dive)]
    pub checkpoint: Checkpoint,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Authoring {
    pub turn: afd_authoring::turn_policy::TurnSettings,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Fleet {
    pub tasks: afd_fleet::task::policy::TaskSettings,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Server {
    #[garde(range(min = 1))]
    pub port: Option<u16>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub api_url: Option<String>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub app_url: Option<String>,
    #[garde(range(min = 1, max = 65_536))]
    pub max_in_flight_requests: Option<usize>,
    #[garde(range(min = 1, max = 65_536))]
    pub max_sse_streams: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Database {
    #[garde(skip)]
    pub api: afd_db::config::PoolOverrides,
    #[garde(skip)]
    pub migrator: afd_db::config::PoolOverrides,
    #[garde(dive)]
    pub migration: Migration,
}

#[derive(Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Migration {
    #[garde(range(min = 1, max = 1000))]
    pub lock_max_attempts: u32,
    #[garde(range(min = 1, max = 300_000 / u64::from(self.lock_max_attempts.max(1))))]
    pub lock_retry_interval_ms: u64,
}

impl Default for Migration {
    fn default() -> Self {
        Self {
            lock_max_attempts: 30,
            lock_retry_interval_ms: 1000,
        }
    }
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Runner {
    #[garde(dive)]
    pub listener: Listener,
    #[garde(dive)]
    pub stream: Stream,
    #[garde(dive)]
    pub lease: Lease,
    #[garde(skip)]
    pub health: super::timing::Health,
    #[garde(skip)]
    pub sandboxes: super::timing::Sandboxes,
    #[garde(dive)]
    pub recovery: Recovery,
    #[garde(dive)]
    pub scheduling: Scheduling,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Listener {
    #[garde(skip)]
    pub address: Option<String>,
    #[garde(skip)]
    pub tls_cert_file: Option<String>,
    #[garde(skip)]
    pub tls_key_file: Option<String>,
    #[garde(range(min = af_stream::DatagramLimit::MIN_BYTES, max = af_stream::DatagramLimit::MAX_BYTES))]
    pub max_udp_payload_bytes: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Stream {
    #[garde(dive)]
    pub control: MessageBuffer,
    #[garde(dive)]
    pub data: MessageBuffer,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct MessageBuffer {
    #[garde(range(min = af_stream::MemoryLimits::MIN_BUFFER_BYTES, max = af_stream::MemoryLimits::MAX_BUFFER_BYTES))]
    pub max_buffered_bytes: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Lease {
    #[garde(range(min = af_core::timing::MIN_LEASE_SECONDS, max = af_core::timing::MAX_LEASE_SECONDS))]
    pub ttl_seconds: Option<u64>,
    #[garde(range(min = af_core::timing::MIN_LEASE_SECONDS + 1, max = af_core::timing::MAX_RUNTIME.as_secs()))]
    pub max_runtime_seconds: Option<u64>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Recovery {
    #[garde(skip)]
    pub resume_from_checkpoint: Option<bool>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Scheduling {
    #[garde(range(min = 1, max = 43_198))]
    pub missed_wake_probe_seconds: Option<u32>,
    #[garde(range(min = 1, max = 64))]
    pub max_concurrent_recovery_claims: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Nats {
    #[garde(skip)]
    pub dev_namespace: Option<String>,
    #[garde(range(min = 1, max = 65_536))]
    pub wake_queue_capacity_fleets: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Identity {
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub issuer: Option<String>,
    #[garde(skip)]
    pub audience: Option<String>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub jwks_url: Option<String>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub clerk_api_base: Option<String>,
    #[garde(skip)]
    pub platform_admin_workspace_id: Option<String>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Connectors {
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub slack_api_url: Option<String>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Telemetry {
    #[garde(skip)]
    pub otlp: af_otlp::PipelineSettings,
    #[garde(skip)]
    pub log_level: Option<LogLevel>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub posthog_host: Option<String>,
    #[garde(inner(custom(|raw, &()| super::urls::public(raw))))]
    pub otlp_endpoint: Option<String>,
    #[garde(skip)]
    pub otlp_protocol: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        })
    }
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Checkpoint {
    #[garde(dive)]
    pub storage: Option<afd_fleet::task::storage::CheckpointStorageConfig>,
}
