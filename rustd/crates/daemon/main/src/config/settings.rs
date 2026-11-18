//! Only deployment inputs accepted in the application file.

use garde::Validate;
use serde::Deserialize;

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ConfigToml {
    #[garde(dive)]
    pub server: Server,
    #[garde(dive)]
    pub database: Database,
    #[garde(dive)]
    pub runner: Runner,
    #[garde(dive)]
    pub bus: Bus,
    #[garde(dive)]
    pub identity: Identity,
    #[garde(dive)]
    pub connectors: Connectors,
    #[garde(dive)]
    pub telemetry: Telemetry,
    #[garde(dive)]
    pub checkpoint: Checkpoint,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Server {
    #[garde(range(min = 1))]
    pub port: Option<u16>,
    #[garde(skip)]
    pub api_url: Option<String>,
    #[garde(skip)]
    pub app_url: Option<String>,
    #[garde(range(min = 1, max = 65_536))]
    pub max_in_flight_requests: Option<usize>,
    #[garde(range(min = 1, max = 65_536))]
    pub max_sse_streams: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Database {
    #[garde(dive)]
    pub api: Pool,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Pool {
    #[garde(range(min = 1))]
    pub max_connections: Option<u32>,
    #[garde(skip)]
    pub min_connections: Option<u32>,
    #[garde(range(min = 1))]
    pub acquire_timeout_ms: Option<u32>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Runner {
    #[garde(dive)]
    pub listener: Listener,
    #[garde(dive)]
    pub stream: Memory,
    #[garde(dive)]
    pub lease: Lease,
    #[garde(dive)]
    pub recovery: Recovery,
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
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Memory {
    #[garde(range(min = 12, max = 512))]
    pub control_mib: Option<usize>,
    #[garde(range(min = 12, max = 512))]
    pub data_mib: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Lease {
    #[garde(range(min = af_core::timing::MIN_LEASE_SECONDS, max = af_core::timing::MAX_LEASE_SECONDS))]
    pub ttl_seconds: Option<u64>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Recovery {
    #[garde(skip)]
    pub resume_from_checkpoint: Option<bool>,
    #[garde(range(min = 1, max = 43_198))]
    pub offer_fallback_seconds: Option<u32>,
    #[garde(range(min = 1, max = 64))]
    pub max_concurrent_claims: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Bus {
    #[garde(skip)]
    pub dev_namespace: Option<String>,
    #[garde(range(min = 1, max = 65_536))]
    pub wake_queue_capacity: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Identity {
    #[garde(skip)]
    pub issuer: Option<String>,
    #[garde(skip)]
    pub audience: Option<String>,
    #[garde(skip)]
    pub jwks_url: Option<String>,
    #[garde(skip)]
    pub clerk_api_base: Option<String>,
    #[garde(skip)]
    pub platform_admin_workspace_id: Option<String>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Connectors {
    #[garde(skip)]
    pub slack_api_url: Option<String>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Telemetry {
    #[garde(skip)]
    pub log_level: Option<String>,
    #[garde(skip)]
    pub posthog_host: Option<String>,
    #[garde(skip)]
    pub otlp_endpoint: Option<String>,
    #[garde(skip)]
    pub otlp_protocol: Option<String>,
    #[garde(range(min = 1))]
    pub otlp_timeout_ms: Option<u32>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Checkpoint {
    #[garde(dive)]
    pub storage: Option<afd_fleet::task::storage::CheckpointStorageConfig>,
}
