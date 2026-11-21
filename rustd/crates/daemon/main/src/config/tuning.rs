//! The daemon's numeric settings, one section per system.
//!
//! Each default is the value the code ran with before the setting existed.
//! Every count, size and timeout refuses zero; a pair whose order matters
//! refuses its wrong order; `[auth]` refuses values that would weaken sign-in.

use std::time::Duration;

use garde::Validate;
use serde::Deserialize;

const MILLISECOND: Duration = Duration::from_millis(1);
const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_mins(1);
const HOUR: Duration = Duration::from_hours(1);
const DAY: Duration = af_core::timing::DAY;

/// Whole `unit`s in a compiled default; every default is far below
/// `u32::MAX` of them.
fn whole(duration: Duration, unit: Duration) -> u32 {
    u32::try_from(duration.as_millis() / unit.as_millis()).unwrap_or(u32::MAX)
}

/// A compiled default count as a key's `u32`; every one is far below
/// `u32::MAX`.
fn count(value: impl TryInto<u32>) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

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
        let pages = afd_http::paging::Pages::default();
        Self {
            default_page_size: pages.list.default_rows(),
            max_page_size: pages.list.max(),
            default_thread_page_size: pages.thread.default_rows(),
            max_thread_page_size: pages.thread.max(),
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
    pub turns_per_assignment_pass: usize,
    /// Longest wait between two repairs of the daemon's bus subscriptions.
    #[garde(range(min = 1))]
    pub max_subscription_repair_backoff_seconds: u32,
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
            turns_per_assignment_pass: afd_fleet::run::RunLimits::default().assign_batch_size,
            max_subscription_repair_backoff_seconds: whole(
                afd_gateway::RecoverySettings::default().max_repair_backoff,
                SECOND,
            ),
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
    pub history_turns_per_page: u32,
    /// Turn checkpoints stored in R2.
    #[garde(dive)]
    pub checkpoints: TurnCheckpointTuning,
}

impl Default for TurnsTuning {
    fn default() -> Self {
        Self {
            max_runs_per_turn: afd_fleet::run::RunLimits::default().max_runs_per_turn,
            max_queued_turns: afd_turns::Budgets::default().deployment_backlog,
            history_turns_per_page: count(afd_fleet::run::history::HISTORY_PAGE_TURNS),
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
    /// How long a checkpoint's uploads or restore may run in total.
    #[garde(range(min = 1))]
    pub transfer_window_minutes: u32,
    /// A checkpoint being restored cannot be deleted during this window.
    #[garde(range(min = 1))]
    pub restore_pin_minutes: u32,
    /// An upload never committed is deletable after this.
    #[garde(range(min = 1))]
    pub orphan_after_minutes: u32,
}

impl Default for TurnCheckpointTuning {
    fn default() -> Self {
        let limits = afd_fleet::run::RunLimits::default().turn_checkpoints;
        Self {
            retention_days: whole(limits.retention, DAY),
            transfer_window_minutes: whole(limits.transfer_window, MINUTE),
            restore_pin_minutes: whole(limits.restore_pin, MINUTE),
            orphan_after_minutes: whole(limits.orphan_after, MINUTE),
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
        let windows = afd_gate::policy::context::ToolWindows::default();
        Self {
            small_model_max_tokens: windows.small_model_max_tokens,
            large_model_min_tokens: windows.large_model_min_tokens,
            small_tool_window_count: windows.small,
            default_tool_window_count: windows.default,
            large_tool_window_count: windows.large,
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
        let run = afd_fleet::run::RunLimits::default();
        Self {
            definition_cache_mib: count(run.definition_cache_bytes / (1024 * 1024)),
            head_cache_count: run.head_cache_count,
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
            price_book_cache_count: afd_fleet::run::RunLimits::default().price_book_cache_count,
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
        let times = afd_fleet::task::policy::TaskTimes::default();
        Self {
            slice_seconds: whole(times.slice, SECOND),
            operation_timeout_seconds: whole(times.operation_timeout, SECOND),
            terminal_retention_days: whole(times.terminal_retention, DAY),
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
    /// How long a runner's presigned upload link stays valid.
    #[garde(range(min = 1))]
    pub upload_link_ttl_minutes: u32,
    /// How long a runner's presigned download link stays valid.
    #[garde(range(min = 1))]
    pub download_link_ttl_minutes: u32,
    /// How long a checkpoint's uploads or restore may run in total.
    #[garde(range(min = 1))]
    pub transfer_window_minutes: u32,
    /// A checkpoint being restored cannot be deleted during this window.
    #[garde(range(min = 1))]
    pub restore_pin_minutes: u32,
    /// An upload never committed is deletable after this.
    #[garde(range(min = 1))]
    pub orphan_after_minutes: u32,
    /// Objects deleted per storage call.
    #[garde(range(min = 1))]
    pub objects_per_delete_call: u32,
}

impl Default for TaskCheckpointTuning {
    fn default() -> Self {
        let times = afd_fleet::task::policy::TaskTimes::default();
        Self {
            keep_generations: times.keep_generations,
            upload_link_ttl_minutes: whole(times.upload_link_ttl, MINUTE),
            download_link_ttl_minutes: whole(times.download_link_ttl, MINUTE),
            transfer_window_minutes: whole(times.transfer_window, MINUTE),
            restore_pin_minutes: whole(times.restore_pin, MINUTE),
            orphan_after_minutes: whole(times.orphan_after, MINUTE),
            objects_per_delete_call: times.delete_batch_size,
        }
    }
}

/// The setup chat.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SetupTuning {
    /// How long one setup request may take before it is answered as busy.
    #[garde(range(min = 1))]
    pub request_timeout_seconds: u32,
    /// A repeated setup request within this returns the first answer.
    #[garde(range(min = 1))]
    pub replay_window_hours: u32,
    /// Drafts read per page by setup maintenance.
    #[garde(range(min = 1))]
    pub maintenance_drafts_per_page: u32,
}

impl Default for SetupTuning {
    fn default() -> Self {
        let setup = afd_setup::tuning::SetupTuning::default();
        Self {
            request_timeout_seconds: whole(setup.budget, SECOND),
            replay_window_hours: whole(setup.replay_window, HOUR),
            maintenance_drafts_per_page: count(setup.scan_batch),
        }
    }
}

/// Sign-in, sessions and tokens; each refuses a value that weakens sign-in.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AuthTuning {
    /// How long a CLI login session waits to be approved.
    #[garde(range(min = 1, max = DAY_SECONDS))]
    pub cli_login_ttl_seconds: u32,
    /// A login code reused within this is refused.
    #[garde(range(min = 1, max = DAY_SECONDS))]
    pub cli_login_code_replay_seconds: u32,
    /// Wrong codes allowed before a login session is closed.
    #[garde(range(min = 1, max = VERIFY_ATTEMPTS_MAX))]
    pub max_cli_login_code_attempts: u32,
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
        let sessions = afd_tenant::session::SessionPolicy::default();
        let identity = crate::identity::Timing::default();
        Self {
            cli_login_ttl_seconds: whole(sessions.ttl, SECOND),
            cli_login_code_replay_seconds: whole(sessions.replay_window, SECOND),
            max_cli_login_code_attempts: u32::from(sessions.max_verify_attempts),
            oauth_state_ttl_minutes: whole(afd_connector::Timing::default().state_ttl, MINUTE),
            identity_timeout_seconds: whole(identity.provider_timeout, SECOND),
            jwks_fetch_timeout_seconds: whole(identity.key_set_timeout, SECOND),
            jwks_cache_hours: whole(identity.key_set_ttl, HOUR),
            jwks_stale_grace_minutes: whole(identity.key_set_stale_grace, MINUTE),
            jwks_min_refresh_seconds: whole(identity.key_set_refresh_min_interval, SECOND),
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
    /// Used when a provider's token states no expiry.
    #[garde(range(min = 1, max = DAY_MINUTES))]
    pub assumed_token_ttl_minutes: u32,
}

impl Default for CredentialsTuning {
    fn default() -> Self {
        Self {
            token_cache_count: afd_credential::credential::broker::MAX_CACHED_TOKENS,
            platform_key_cache_minutes: whole(
                afd_credential::provider::cache::PLATFORM_KEY_TTL,
                MINUTE,
            ),
            assumed_token_ttl_minutes: whole(
                afd_credential::credential::Lifetimes::default().assumed_access,
                MINUTE,
            ),
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
            installation_token_ttl_minutes: whole(
                afd_credential::credential::Lifetimes::default().installation_token,
                MINUTE,
            ),
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
    pub thread_read_timeout_ms: u32,
}

impl Default for SlackTuning {
    fn default() -> Self {
        let deadlines = afd_outbound::SlackDeadlines::default();
        Self {
            post_timeout_seconds: whole(deadlines.post, SECOND),
            answer_check_timeout_seconds: whole(deadlines.answer_check, SECOND),
            thread_read_timeout_ms: whole(afd_connector::Timing::default().slack_read, MILLISECOND),
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
            send_timeout_seconds: whole(afd_mail::MAIL_SEND_DEADLINE, SECOND),
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
            attempt_timeout_ms: whole(afd_cron::qstash::ATTEMPT_TIMEOUT, MILLISECOND),
            call_budget_seconds: whole(afd_cron::qstash::CALL_BUDGET, SECOND),
            max_attempts: count(afd_cron::qstash::ATTEMPTS),
            max_retry_delay_ms: whole(afd_cron::qstash::MAX_DELAY, MILLISECOND),
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
    pub drain_timeout: Duration,
    pub task_join_timeout: Duration,
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
            task_join_timeout: super::settings::secs(server.background_task_join_timeout_seconds),
            max_live_activity_notices: server.max_live_activity_notices,
        }
    }
}

/// `[telemetry]`'s metric-table keys.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MetricTuning {
    pub max_metric_series: usize,
    pub metric_max_age: Duration,
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
            ttl: super::settings::secs(self.cli_login_ttl_seconds),
            replay_window: super::settings::secs(self.cli_login_code_replay_seconds),
            // Garde holds this at or below `VERIFY_ATTEMPTS_MAX`.
            max_verify_attempts: u8::try_from(self.max_cli_login_code_attempts).unwrap_or(u8::MAX),
        }
    }
}

impl Tuning {
    /// The credential broker's token lifetimes and cache size.
    pub(crate) fn minting(&self) -> crate::credentials::Minting {
        crate::credentials::Minting {
            lifetimes: afd_credential::credential::Lifetimes {
                assumed_access: super::settings::mins(self.credentials.assumed_token_ttl_minutes),
                installation_token: super::settings::mins(
                    self.github.installation_token_ttl_minutes,
                ),
            },
            cached_tokens: self.credentials.token_cache_count,
        }
    }
}

/// A configured count of seconds as a duration.
pub(crate) fn seconds(value: u32) -> Duration {
    super::settings::secs(value)
}

/// A configured count of minutes as a duration.
pub(crate) fn minutes(value: u32) -> Duration {
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
            slack_read: Duration::from_millis(u64::from(self.slack.thread_read_timeout_ms)),
        }
    }
}

impl QstashTuning {
    /// The scheduler's retry policy, or `None` when it cannot end inside
    /// `call_budget_seconds`.
    pub(crate) fn policy(&self) -> Option<afd_cron::qstash::RetryPolicy> {
        afd_cron::qstash::RetryPolicy::new(
            usize::try_from(self.max_attempts).ok()?,
            Duration::from_millis(u64::from(self.attempt_timeout_ms)),
            Duration::from_millis(u64::from(self.max_retry_delay_ms)),
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
