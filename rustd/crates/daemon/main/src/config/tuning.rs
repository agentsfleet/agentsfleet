//! The daemon's numeric settings, one section per system.
//!
//! Each default is the value the code ran with before the setting existed.
//! Every count, size and timeout refuses zero; a pair whose order matters
//! refuses its wrong order; `[auth]` refuses values that would weaken sign-in.

use garde::Validate;
use serde::Deserialize;

/// One hour, the ceiling GitHub sets on an installation token.
const HOUR_MINUTES: u32 = 60;

/// One day: no sign-in lifetime may outlast it.
const DAY_SECONDS: u32 = 86_400;

/// One day, in minutes.
const DAY_MINUTES: u32 = 1_440;

/// One day, in hours.
const DAY_HOURS: u32 = 24;

/// A call to the identity provider that takes longer than this has failed.
const IDENTITY_CALL_SECONDS_MAX: u32 = 60;

/// Wrong codes past this make a login code guessable.
const VERIFY_ATTEMPTS_MAX: u32 = 10;

/// List reads.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_field_names,
    reason = "the fields are the approved file's key names"
)]
pub(crate) struct ApiTuning {
    /// Rows a list returns when the caller names no size.
    #[garde(range(min = 1, max = self.max_page_size))]
    pub default_page_size: u32,
    /// Most rows any list returns at once.
    #[garde(range(min = 1))]
    pub max_page_size: u32,
    /// Messages a fleet's chat returns when the caller names no size.
    #[garde(range(min = 1, max = self.max_thread_page_size))]
    pub default_thread_page_size: u32,
    /// Most messages a fleet's chat returns at once.
    #[garde(range(min = 1))]
    pub max_thread_page_size: u32,
}

impl Default for ApiTuning {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 100,
            default_thread_page_size: 25,
            max_thread_page_size: 100,
        }
    }
}

/// The link between the daemon and runners.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RunnersTuning {
    /// Largest message on the runner link; sent to runners when they connect.
    #[garde(range(min = 1, max = MESSAGE_MIB_MAX))]
    pub max_message_mib: u32,
    /// Largest control message on the runner link; sent to runners when they connect.
    #[garde(range(min = 1, max = CONTROL_MESSAGE_MIB_MAX))]
    pub max_control_message_mib: u32,
    /// Waiting turns considered per assignment pass.
    #[garde(range(min = 1))]
    pub assign_batch_size: usize,
    /// Longest wait before retrying a broken runner connection repair.
    #[garde(range(min = 1))]
    pub max_repair_backoff_seconds: u32,
}

/// The runner link's framing holds no larger message than this.
const MESSAGE_MIB_MAX: u32 = mib(af_wire::protocol::MESSAGE_BYTES_MAX);

/// The control stream's memory pool holds no larger message than this.
const CONTROL_MESSAGE_MIB_MAX: u32 = mib(af_wire::protocol::CONTROL_MESSAGE_BYTES_MAX);

#[expect(
    clippy::cast_possible_truncation,
    reason = "a frame ceiling in MiB is far below u32::MAX"
)]
const fn mib(bytes: usize) -> u32 {
    (bytes / (1024 * 1024)) as u32
}

impl Default for RunnersTuning {
    fn default() -> Self {
        Self {
            max_message_mib: MESSAGE_MIB_MAX,
            max_control_message_mib: CONTROL_MESSAGE_MIB_MAX,
            assign_batch_size: 64,
            max_repair_backoff_seconds: 5,
        }
    }
}

/// Queued and running turns.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TurnsTuning {
    /// After this many runs a turn fails.
    #[garde(range(min = 1))]
    pub max_runs_per_turn: u32,
    /// Queued turns across the deployment; past this new messages are refused.
    #[garde(range(min = 1))]
    pub max_queued_turns: u64,
    /// Earlier turns read per page when a run is given its chat history.
    #[garde(range(min = 1))]
    pub history_page_size: u32,
    /// Turn checkpoints stored in R2.
    #[garde(dive)]
    pub checkpoints: TurnCheckpointTuning,
}

impl Default for TurnsTuning {
    fn default() -> Self {
        Self {
            max_runs_per_turn: 3,
            max_queued_turns: 100_000,
            history_page_size: 100,
            checkpoints: TurnCheckpointTuning::default(),
        }
    }
}

/// Turn checkpoints stored in R2.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TurnCheckpointTuning {
    /// A settled turn's checkpoint is deleted after this.
    #[garde(range(min = 1))]
    pub retention_days: u32,
    /// How long a runner's upload link stays valid.
    #[garde(range(min = 1))]
    pub upload_url_ttl_minutes: u32,
    /// A checkpoint being restored cannot be deleted during this window.
    #[garde(range(min = 1))]
    pub restore_pin_minutes: u32,
    /// An upload never committed is deletable after this.
    #[garde(range(min = 1))]
    pub orphan_after_minutes: u32,
}

impl Default for TurnCheckpointTuning {
    fn default() -> Self {
        Self {
            retention_days: 7,
            upload_url_ttl_minutes: 15,
            restore_pin_minutes: 2,
            orphan_after_minutes: 60,
        }
    }
}

/// How much tool history a run keeps in its context.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ContextTuning {
    /// A model with a window at or below this uses the small tool window.
    #[garde(range(min = 1, max = self.large_model_min_tokens.saturating_sub(1)))]
    pub small_model_max_tokens: u32,
    /// A model with a window at or above this uses the large tool window.
    #[garde(range(min = 1))]
    pub large_model_min_tokens: u32,
    /// Recent tool calls kept for a small model.
    #[garde(range(min = 1))]
    pub small_tool_window_count: u32,
    /// Recent tool calls kept for a model in between.
    #[garde(range(min = 1))]
    pub default_tool_window_count: u32,
    /// Recent tool calls kept for a large model.
    #[garde(range(min = 1))]
    pub large_tool_window_count: u32,
}

impl Default for ContextTuning {
    fn default() -> Self {
        Self {
            small_model_max_tokens: 200_000,
            large_model_min_tokens: 1_000_000,
            small_tool_window_count: 10,
            default_tool_window_count: 20,
            large_tool_window_count: 30,
        }
    }
}

/// Fleet definitions.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct FleetsTuning {
    /// Memory for parsed fleet definitions.
    #[garde(range(min = 1))]
    pub definition_cache_mib: u32,
    /// Fleets whose latest revision is kept in memory.
    #[garde(range(min = 1))]
    pub head_cache_count: u64,
}

impl Default for FleetsTuning {
    fn default() -> Self {
        Self {
            definition_cache_mib: 64,
            head_cache_count: 16_384,
        }
    }
}

/// Money.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct BillingTuning {
    /// Model prices kept in memory for metering runs.
    #[garde(range(min = 1))]
    pub price_book_cache_count: u64,
}

impl Default for BillingTuning {
    fn default() -> Self {
        Self {
            price_book_cache_count: 1_024,
        }
    }
}

/// Durable tasks.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TasksTuning {
    /// A task runs in slices of this length; its funds are checked before each.
    #[garde(range(min = 1))]
    pub slice_seconds: u32,
    /// How long one task operation may take.
    #[garde(range(min = 1))]
    pub operation_timeout_seconds: u32,
    /// A finished task's records are kept this long.
    #[garde(range(min = 1))]
    pub terminal_retention_days: u32,
    /// Task checkpoints stored in R2.
    #[garde(dive)]
    pub checkpoints: TaskCheckpointTuning,
}

impl Default for TasksTuning {
    fn default() -> Self {
        Self {
            slice_seconds: 600,
            operation_timeout_seconds: 30,
            terminal_retention_days: 7,
            checkpoints: TaskCheckpointTuning::default(),
        }
    }
}

/// Task checkpoints stored in R2.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TaskCheckpointTuning {
    /// Newest checkpoints kept per task.
    #[garde(range(min = 1))]
    pub keep_generations: u32,
    /// How long a runner's upload link stays valid.
    #[garde(range(min = 1))]
    pub upload_url_ttl_minutes: u32,
    /// How long a runner's download link stays valid.
    #[garde(range(min = 1))]
    pub download_url_ttl_minutes: u32,
    /// A checkpoint being restored cannot be deleted during this window.
    #[garde(range(min = 1))]
    pub restore_pin_minutes: u32,
    /// An upload never committed is deletable after this.
    #[garde(range(min = 1))]
    pub orphan_after_minutes: u32,
    /// Objects deleted per storage call.
    #[garde(range(min = 1))]
    pub delete_batch_size: u32,
}

impl Default for TaskCheckpointTuning {
    fn default() -> Self {
        Self {
            keep_generations: 3,
            upload_url_ttl_minutes: 15,
            download_url_ttl_minutes: 5,
            restore_pin_minutes: 30,
            orphan_after_minutes: 60,
            delete_batch_size: 256,
        }
    }
}

/// The setup chat.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SetupTuning {
    /// How long one setup request may take before it is answered as busy.
    #[garde(range(min = 1))]
    pub budget_seconds: u32,
    /// A repeated setup request within this returns the first answer.
    #[garde(range(min = 1))]
    pub replay_window_hours: u32,
    /// Drafts read per page by setup maintenance.
    #[garde(range(min = 1))]
    pub scan_batch_size: u32,
}

impl Default for SetupTuning {
    fn default() -> Self {
        Self {
            budget_seconds: 15,
            replay_window_hours: 24,
            scan_batch_size: 64,
        }
    }
}

/// Sign-in, sessions and tokens; each refuses a value that weakens sign-in.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AuthTuning {
    /// How long a CLI login session waits to be approved.
    #[garde(range(min = 1, max = DAY_SECONDS))]
    pub session_ttl_seconds: u32,
    /// A login code reused within this is refused.
    #[garde(range(min = 1, max = DAY_SECONDS))]
    pub session_replay_seconds: u32,
    /// Wrong codes allowed before a login session is closed.
    #[garde(range(min = 1, max = VERIFY_ATTEMPTS_MAX))]
    pub max_session_verify_attempts: u32,
    /// How long a minted access token lives.
    #[garde(range(min = 1, max = DAY_MINUTES))]
    pub access_token_ttl_minutes: u32,
    /// How long a connector sign-in link stays valid.
    #[garde(range(min = 1, max = DAY_MINUTES))]
    pub oauth_state_ttl_minutes: u32,
    /// How long a call to the identity provider may take.
    #[garde(range(min = 1, max = IDENTITY_CALL_SECONDS_MAX))]
    pub identity_timeout_seconds: u32,
    /// How long fetching the identity provider's signing keys may take.
    #[garde(range(min = 1, max = IDENTITY_CALL_SECONDS_MAX))]
    pub jwks_fetch_timeout_seconds: u32,
    /// How long signing keys are trusted before refetching.
    #[garde(range(min = 1, max = DAY_HOURS))]
    pub jwks_cache_hours: u32,
    /// Keys still accepted this long after a failed refetch.
    #[garde(range(min = 1, max = DAY_MINUTES))]
    pub jwks_stale_grace_minutes: u32,
    /// Signing keys are refetched no more often than this.
    #[garde(range(min = 1, max = DAY_SECONDS))]
    pub jwks_min_refresh_seconds: u32,
}

impl Default for AuthTuning {
    fn default() -> Self {
        Self {
            session_ttl_seconds: 300,
            session_replay_seconds: 60,
            max_session_verify_attempts: 5,
            access_token_ttl_minutes: 5,
            oauth_state_ttl_minutes: 10,
            identity_timeout_seconds: 5,
            jwks_fetch_timeout_seconds: 5,
            jwks_cache_hours: 6,
            jwks_stale_grace_minutes: 15,
            jwks_min_refresh_seconds: 30,
        }
    }
}

/// Provider and connector credentials held by the daemon.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CredentialsTuning {
    /// Minted tokens kept in memory.
    #[garde(range(min = 1))]
    pub token_cache_count: u64,
    /// How long a platform model key is reused before re-reading the vault.
    #[garde(range(min = 1))]
    pub platform_key_cache_minutes: u32,
}

impl Default for CredentialsTuning {
    fn default() -> Self {
        Self {
            token_cache_count: 8_192,
            platform_key_cache_minutes: 5,
        }
    }
}

/// GitHub's app tokens.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct GithubTuning {
    /// How long a GitHub app token is used; GitHub's own maximum is 60.
    #[garde(range(min = 1, max = HOUR_MINUTES))]
    pub installation_token_ttl_minutes: u32,
}

impl Default for GithubTuning {
    fn default() -> Self {
        Self {
            installation_token_ttl_minutes: HOUR_MINUTES,
        }
    }
}

/// Slack's calls.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SlackTuning {
    /// How long posting an answer to Slack may take.
    #[garde(range(min = 1))]
    pub post_timeout_seconds: u32,
    /// How long checking whether a thread was already answered may take.
    #[garde(range(min = 1))]
    pub answer_check_timeout_seconds: u32,
    /// How long reading a Slack thread may take.
    #[garde(range(min = 1))]
    pub read_timeout_ms: u32,
}

impl Default for SlackTuning {
    fn default() -> Self {
        Self {
            post_timeout_seconds: 5,
            answer_check_timeout_seconds: 3,
            read_timeout_ms: 1_500,
        }
    }
}

/// Invite email.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct MailTuning {
    /// How long sending one email may take.
    #[garde(range(min = 1))]
    pub send_timeout_seconds: u32,
}

impl Default for MailTuning {
    fn default() -> Self {
        Self {
            send_timeout_seconds: 10,
        }
    }
}

/// Fleet schedules.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct QstashTuning {
    /// How long one call to `QStash` may take.
    #[garde(range(min = 1))]
    pub attempt_timeout_ms: u32,
    /// Total time for a `QStash` call, retries included.
    #[garde(range(min = 1))]
    pub call_budget_seconds: u32,
    /// Tries per `QStash` call.
    #[garde(range(min = 1))]
    pub max_attempts: u32,
    /// Longest pause between tries.
    #[garde(range(min = 1))]
    pub max_retry_delay_ms: u32,
}

impl Default for QstashTuning {
    fn default() -> Self {
        Self {
            attempt_timeout_ms: 2_500,
            call_budget_seconds: 10,
            max_attempts: 3,
            max_retry_delay_ms: 800,
        }
    }
}

/// What the plane reads: every section above, checked, plus the keys of the
/// sections that predate them.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Tuning {
    pub server: ServerTimes,
    pub api: ApiTuning,
    pub runners: RunnersTuning,
    pub turns: TurnsTuning,
    pub context: ContextTuning,
    pub fleets: FleetsTuning,
    pub billing: BillingTuning,
    pub tasks: TasksTuning,
    pub setup: SetupTuning,
    pub auth: AuthTuning,
    pub credentials: CredentialsTuning,
    pub github: GithubTuning,
    pub slack: SlackTuning,
    pub mail: MailTuning,
    pub metrics: MetricTuning,
    pub wake: afd_bus::wake::QueueBounds,
}

/// `[server]`'s timing and fan-out keys.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ServerTimes {
    pub drain_timeout: std::time::Duration,
    pub task_join_timeout: std::time::Duration,
    pub max_live_activity_notices: usize,
}

impl Default for ServerTimes {
    fn default() -> Self {
        Self::from(&super::settings::Server::default())
    }
}

impl From<&super::settings::Server> for ServerTimes {
    fn from(server: &super::settings::Server) -> Self {
        Self {
            drain_timeout: super::settings::secs(server.drain_timeout_seconds),
            task_join_timeout: super::settings::secs(server.task_join_timeout_seconds),
            max_live_activity_notices: server.max_live_activity_notices,
        }
    }
}

/// `[telemetry]`'s metric-table keys.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MetricTuning {
    pub max_metric_series: usize,
    pub metric_max_age: std::time::Duration,
}

impl Default for MetricTuning {
    fn default() -> Self {
        Self::from(&super::settings::Telemetry::default())
    }
}

impl From<&super::settings::Telemetry> for MetricTuning {
    fn from(telemetry: &super::settings::Telemetry) -> Self {
        Self {
            max_metric_series: telemetry.max_metric_series,
            metric_max_age: super::settings::secs(telemetry.metric_max_age_seconds),
        }
    }
}

impl From<&super::settings::ConfigToml> for Tuning {
    fn from(settings: &super::settings::ConfigToml) -> Self {
        Self {
            server: ServerTimes::from(&settings.server),
            api: settings.api,
            runners: settings.runners,
            turns: settings.turns,
            context: settings.context,
            fleets: settings.fleets,
            billing: settings.billing,
            tasks: settings.tasks,
            setup: settings.setup,
            auth: settings.auth,
            credentials: settings.credentials,
            github: settings.connectors.github,
            slack: settings.connectors.slack,
            mail: settings.connectors.mail,
            metrics: MetricTuning::from(&settings.telemetry),
            wake: afd_bus::wake::QueueBounds {
                default: settings.nats.default_wake_queue_size,
                max: settings.nats.max_wake_queue_size,
            },
        }
    }
}

impl AuthTuning {
    /// The device-login store's lifetimes and wrong-code allowance.
    pub(crate) fn sessions(&self) -> afd_tenant::session::SessionPolicy {
        afd_tenant::session::SessionPolicy {
            ttl: super::settings::secs(self.session_ttl_seconds),
            replay_window: super::settings::secs(self.session_replay_seconds),
            // Garde holds this at or below `VERIFY_ATTEMPTS_MAX`.
            max_verify_attempts: u8::try_from(self.max_session_verify_attempts).unwrap_or(u8::MAX),
        }
    }
}

impl Tuning {
    /// The credential broker's token lifetimes and cache size.
    pub(crate) fn minting(&self) -> crate::credentials::Minting {
        crate::credentials::Minting {
            lifetimes: afd_credential::credential::Lifetimes {
                assumed_access: super::settings::mins(self.auth.access_token_ttl_minutes),
                installation_token: super::settings::mins(
                    self.github.installation_token_ttl_minutes,
                ),
            },
            cached_tokens: self.credentials.token_cache_count,
        }
    }
}

/// A configured count of seconds as a duration.
pub(crate) fn seconds(value: u32) -> std::time::Duration {
    super::settings::secs(value)
}

/// A configured count of minutes as a duration.
pub(crate) fn minutes(value: u32) -> std::time::Duration {
    super::settings::mins(value)
}

impl Tuning {
    /// The Slack poster's post and already-answered deadlines.
    pub(crate) fn slack_deadlines(&self) -> afd_outbound::SlackDeadlines {
        afd_outbound::SlackDeadlines {
            post: seconds(self.slack.post_timeout_seconds),
            answer_check: seconds(self.slack.answer_check_timeout_seconds),
        }
    }

    /// The connect flow's state lifetime and Slack thread-read bound.
    pub(crate) fn connect(&self) -> afd_connector::Timing {
        afd_connector::Timing {
            state_ttl: minutes(self.auth.oauth_state_ttl_minutes),
            slack_read: std::time::Duration::from_millis(u64::from(self.slack.read_timeout_ms)),
        }
    }
}

impl QstashTuning {
    /// The scheduler's retry policy, or `None` when it cannot end inside
    /// `call_budget_seconds`.
    pub(crate) fn policy(&self) -> Option<afd_cron::qstash::RetryPolicy> {
        afd_cron::qstash::RetryPolicy::new(
            usize::try_from(self.max_attempts).ok()?,
            std::time::Duration::from_millis(u64::from(self.attempt_timeout_ms)),
            std::time::Duration::from_millis(u64::from(self.max_retry_delay_ms)),
            seconds(self.call_budget_seconds),
        )
    }
}

impl Tuning {
    /// The identity provider's call timeouts and signing-key lifetimes.
    pub(crate) fn identity(&self) -> crate::identity::Timing {
        crate::identity::Timing {
            provider_timeout: seconds(self.auth.identity_timeout_seconds),
            key_set_timeout: seconds(self.auth.jwks_fetch_timeout_seconds),
            key_set_ttl: minutes(self.auth.jwks_cache_hours).saturating_mul(60),
            key_set_stale_grace: minutes(self.auth.jwks_stale_grace_minutes),
            key_set_refresh_min_interval: seconds(self.auth.jwks_min_refresh_seconds),
        }
    }
}

impl Tuning {
    /// The per-runner metric table's size and the catalogue snapshot's age.
    pub(crate) const fn metric_limits(&self) -> af_observability::producers::Limits {
        af_observability::producers::Limits {
            max_runner_series: self.metrics.max_metric_series,
            model_snapshot_max_age: self.metrics.metric_max_age,
        }
    }
}
