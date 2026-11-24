//! Only deployment inputs accepted in the application file.

use garde::Validate;
use serde::Deserialize;

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ConfigToml {
    #[garde(dive)]
    pub outbound: Outbound,
    #[garde(skip)]
    pub fleet: Fleet,
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
    #[garde(dive)]
    pub api: super::tuning::ApiTuning,
    #[garde(dive)]
    pub runners: super::tuning::RunnersTuning,
    #[garde(dive)]
    pub turns: super::tuning::TurnsTuning,
    #[garde(dive)]
    pub context: super::tuning::ContextTuning,
    #[garde(dive)]
    pub fleets: super::tuning::FleetsTuning,
    #[garde(dive)]
    pub billing: super::tuning::BillingTuning,
    #[garde(dive)]
    pub durable_tasks: super::tuning::DurableTasksTuning,
    #[garde(dive)]
    pub setup: super::tuning::SetupTuning,
    #[garde(dive)]
    pub auth: super::tuning::AuthTuning,
    #[garde(dive)]
    pub credentials: super::tuning::CredentialsTuning,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Outbound {
    /// Validated once, where `policies::read` turns it into the retry policy.
    #[garde(skip)]
    pub retries: afd_outbound::retry::RetrySettings,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Fleet {
    pub durable_tasks: afd_fleet::durable_task::policy::DurableTaskSettings,
}

#[derive(Deserialize, Validate)]
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
    /// On shutdown, how long open requests get to finish; longer than a
    /// runner answer already in flight may take.
    #[garde(range(min = DRAIN_SECONDS_MIN))]
    pub drain_timeout_seconds: u32,
    /// On shutdown, how long background tasks get to stop; longer than the
    /// telemetry flush, so the flush is joined rather than abandoned.
    #[garde(range(min = JOIN_SECONDS_MIN))]
    pub background_task_join_timeout_seconds: u32,
    /// Live-activity updates sent to dashboards at the same time.
    #[garde(range(min = 1))]
    pub max_live_activity_notices: usize,
    /// Largest request body a route reads; a larger one is refused 413.
    #[garde(range(min = 1))]
    pub max_request_body_mib: usize,
    /// How long a client gets to send a request's head, and then its body;
    /// past it the connection is dropped. A response is never timed.
    #[garde(range(min = 1))]
    pub read_timeout_seconds: u32,
}

/// A runner answer in flight when the drain starts must finish inside it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a five-second answer timeout is far below u32::MAX seconds"
)]
const DRAIN_SECONDS_MIN: u32 = af_core::timing::STREAM_ANSWER_TIMEOUT.as_secs() as u32 + 1;

/// The shutdown flush must finish inside the join.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an eight-second flush budget is far below u32::MAX seconds"
)]
const JOIN_SECONDS_MIN: u32 = crate::serve::FLUSH_BUDGET.as_secs() as u32 + 1;

impl Default for Server {
    fn default() -> Self {
        Self {
            port: None,
            api_url: None,
            app_url: None,
            max_in_flight_requests: None,
            max_sse_streams: None,
            drain_timeout_seconds: seconds(crate::serve::DRAIN_TIMEOUT),
            background_task_join_timeout_seconds: seconds(crate::supervisor::JOIN_TIMEOUT),
            max_live_activity_notices: afd_turns::view::activity::DEFAULT_ACTIVITY_NOTICES,
            max_request_body_mib: afd_http::body::DEFAULT_MAX_REQUEST_BODY_BYTES
                / afd_http::body::MIB,
            read_timeout_seconds: seconds(afd_api::DEFAULT_READ_TIMEOUT),
        }
    }
}

/// The pool settings below are defaults: `DATABASE_POOL_SIZE`,
/// `DATABASE_MIN_POOL_SIZE` and their `_API` / `_MIGRATOR` forms still
/// override `max_connections` and `min_connections`, because deployments set
/// them, and a `[database.api]` or `[database.migrator]` value overrides both.
#[derive(Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Database {
    #[garde(skip)]
    pub api: afd_db::config::PoolOverrides,
    #[garde(skip)]
    pub migrator: afd_db::config::PoolOverrides,
    #[garde(dive)]
    pub migration: Migration,
    /// Connections this daemon may hold.
    #[garde(range(min = 1, max = 65_535))]
    pub max_connections: u32,
    /// Connections kept open so the first requests after idle are fast.
    /// Unset, a quarter of the resolved ceiling: 5 of the default 20.
    #[garde(inner(range(min = 1, max = self.max_connections)))]
    pub min_connections: Option<u32>,
    /// How long a request waits for a free connection before failing.
    #[garde(range(min = 1))]
    pub acquire_timeout_seconds: u32,
    /// How long opening a new connection may take.
    #[garde(range(min = 1))]
    pub connect_timeout_seconds: u32,
    /// An unused connection is closed after this.
    #[garde(range(min = 1))]
    pub idle_timeout_minutes: u32,
    /// Every connection is replaced after this, so a failover is picked up.
    #[garde(range(min = 1))]
    pub max_connection_lifetime_minutes: u32,
}

impl Default for Database {
    fn default() -> Self {
        let pool = afd_db::config::PoolDefaults::default();
        Self {
            api: afd_db::config::PoolOverrides::default(),
            migrator: afd_db::config::PoolOverrides::default(),
            migration: Migration::default(),
            max_connections: pool.max_connections,
            min_connections: pool.min_connections,
            acquire_timeout_seconds: seconds(pool.acquire_timeout),
            connect_timeout_seconds: seconds(pool.connect_timeout),
            idle_timeout_minutes: seconds(pool.idle_timeout) / 60,
            max_connection_lifetime_minutes: seconds(pool.max_lifetime) / 60,
        }
    }
}

impl Database {
    /// The pool defaults the role overrides and the environment sit above.
    pub(super) fn pool_defaults(&self) -> afd_db::config::PoolDefaults {
        afd_db::config::PoolDefaults {
            max_connections: self.max_connections,
            min_connections: self.min_connections,
            acquire_timeout: secs(self.acquire_timeout_seconds),
            connect_timeout: secs(self.connect_timeout_seconds),
            idle_timeout: mins(self.idle_timeout_minutes),
            max_lifetime: mins(self.max_connection_lifetime_minutes),
            ..afd_db::config::PoolDefaults::default()
        }
    }
}

/// Whole seconds of a compiled default, which are all far below `u32::MAX`.
fn seconds(duration: std::time::Duration) -> u32 {
    u32::try_from(duration.as_secs()).unwrap_or(u32::MAX)
}

pub(super) fn secs(value: u32) -> std::time::Duration {
    std::time::Duration::from_secs(u64::from(value))
}

pub(super) fn mins(value: u32) -> std::time::Duration {
    secs(value).saturating_mul(60)
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
    pub run: Run,
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
    pub control: ControlBuffer,
    #[garde(dive)]
    pub data: DataBuffer,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ControlBuffer {
    #[garde(range(min = af_stream::MemoryLimits::MIN_CONTROL_BUFFER_BYTES, max = af_stream::MemoryLimits::MAX_BUFFER_BYTES))]
    pub max_buffered_bytes: Option<usize>,
}

/// Its floor holds one maximum work or report frame in each direction.
#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct DataBuffer {
    #[garde(range(min = af_stream::MemoryLimits::MIN_DATA_BUFFER_BYTES, max = af_stream::MemoryLimits::MAX_BUFFER_BYTES))]
    pub max_buffered_bytes: Option<usize>,
}

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Run {
    #[garde(range(min = afd_runner::timing::MIN_RUN_SECONDS, max = af_core::timing::MAX_RUN_SECONDS))]
    pub ttl_seconds: Option<u64>,
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
    #[garde(range(min = 1, max = 64))]
    pub max_concurrent_recovery_assignments: Option<usize>,
}

#[derive(Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct Nats {
    #[garde(skip)]
    pub dev_namespace: Option<String>,
    #[garde(range(min = 1, max = self.max_wake_queue_size))]
    pub wake_queue_capacity_fleets: Option<usize>,
    /// Wake-ups held per daemon when nothing else sets the queue's size.
    #[garde(range(min = 1, max = self.max_wake_queue_size))]
    pub default_wake_queue_size: usize,
    /// The most the wake queue may be raised to.
    #[garde(range(min = 1))]
    pub max_wake_queue_size: usize,
}

impl Default for Nats {
    fn default() -> Self {
        let bounds = afd_bus::wake::QueueBounds::default();
        Self {
            dev_namespace: None,
            wake_queue_capacity_fleets: None,
            default_wake_queue_size: bounds.default,
            max_wake_queue_size: bounds.max,
        }
    }
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
    #[garde(dive)]
    pub github: super::tuning::GithubTuning,
    #[garde(dive)]
    pub slack: super::tuning::SlackTuning,
    #[garde(dive)]
    pub mail: super::tuning::MailTuning,
    #[garde(dive)]
    pub qstash: super::tuning::QstashTuning,
}

#[derive(Deserialize, Validate)]
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
    /// Distinct metric series kept; past this new ones are dropped.
    #[garde(range(min = 1))]
    pub max_metric_series: usize,
    /// A metric not updated within this is dropped.
    #[garde(range(min = 1))]
    pub metric_max_age_seconds: u32,
}

impl Default for Telemetry {
    fn default() -> Self {
        let metrics = af_observability::producers::Limits::default();
        Self {
            otlp: af_otlp::PipelineSettings::default(),
            log_level: None,
            posthog_host: None,
            otlp_endpoint: None,
            otlp_protocol: None,
            max_metric_series: metrics.max_runner_series,
            metric_max_age_seconds: seconds(metrics.model_snapshot_max_age),
        }
    }
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
    pub storage: Option<afd_fleet::durable_task::storage::CheckpointStorageConfig>,
}
