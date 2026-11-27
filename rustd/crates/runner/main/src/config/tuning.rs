//! The runner's numeric settings, one section per system.
//!
//! Each default is the value the code ran with before the setting existed.
//! Every count, size and timeout refuses zero; a pair whose order matters
//! refuses its wrong order; a value a fixed bound elsewhere cannot follow
//! refuses past that bound.

use std::time::Duration;

use garde::Validate;
use serde::Deserialize;

/// One mebibyte.
const MIB: u64 = 1024 * 1024;

/// One kibibyte.
const KIB: usize = 1024;

/// One mebibyte, as a size in memory.
const MIB_USIZE: usize = KIB * KIB;

/// The longest timeout a command may run under: tokio's timer holds no longer
/// a sleep than `2^31 - 1` milliseconds. The run's lease does not bound it,
/// because the supervisor renews the lease while a tool call runs.
const SHELL_TIMEOUT_SECONDS_MAX: u64 = guffbolt::tools::process::oneshot::TIMEOUT_MAX.as_secs();

/// Room is never made from the most recently used shell processes, so a cap
/// at or below their count would not hold.
const SHELL_PROCESSES_MIN: usize = guffbolt::tools::process::sessions::SESSIONS_PROTECTED + 1;

/// A restore checks a checkpoint's children against this depth.
const SUBAGENT_DEPTH_MAX: u8 = afr_agent::RunLimits::DEFAULT.caps.depth;

/// One live-tail batch in flight and one queued.
const ACTIVITY_BATCHES_MIN: usize = 2;

/// The shortest frame a full directory listing still fits in.
const FRAME_MIB_MIN: usize = afr_executor::Sizes::LISTING_BYTES / MIB_USIZE + 1;

/// The box each run executes in.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct SandboxTuning {
    /// Memory one run may use.
    #[garde(range(min = 1))]
    pub memory_mib: u64,
    /// Disk one run may use.
    #[garde(range(min = 1))]
    pub disk_mib: u64,
    /// Disk read and write speed one run may use.
    #[garde(range(min = 1))]
    pub io_mib_per_second: u64,
    /// How long a new sandbox may take to start.
    #[garde(range(min = 1))]
    pub ready_timeout_seconds: u64,
}

impl Default for SandboxTuning {
    fn default() -> Self {
        let limits = afr_engine::Limits::DEFAULT;
        Self {
            memory_mib: limits.memory_bytes / MIB,
            disk_mib: limits.disk_bytes / MIB,
            io_mib_per_second: afr_cgroup::DEFAULT_IO_BYTES_PER_SECOND / MIB,
            ready_timeout_seconds: afr_engine::DEFAULT_READY_TIMEOUT.as_secs(),
        }
    }
}

impl SandboxTuning {
    /// What every run's sandbox enforces.
    pub(super) fn limits(&self) -> afr_engine::Limits {
        afr_engine::Limits {
            memory_bytes: self.memory_mib.saturating_mul(MIB),
            disk_bytes: self.disk_mib.saturating_mul(MIB),
            ..afr_engine::Limits::DEFAULT
        }
    }

    /// How long a new sandbox may take to answer before its run is refused.
    pub(super) const fn ready_timeout(&self) -> Duration {
        Duration::from_secs(self.ready_timeout_seconds)
    }

    /// Disk bytes a run's sandbox may read, and write, per second.
    pub(super) const fn io_bytes_per_second(&self) -> u64 {
        self.io_mib_per_second.saturating_mul(MIB)
    }
}

/// What a run's tools may do.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ToolsTuning {
    /// How long a shell command runs when the model names no timeout.
    #[garde(range(min = 1, max = SHELL_TIMEOUT_SECONDS_MAX))]
    pub default_shell_timeout_seconds: u64,
    /// Shortest wait for a running command's output.
    #[garde(range(min = 1, max = self.max_command_output_wait_ms))]
    pub min_command_output_wait_ms: u64,
    /// Longest wait for a running command's output.
    #[garde(range(min = 1))]
    pub max_command_output_wait_ms: u64,
    /// Interactive shell processes one run may keep open.
    #[garde(range(min = SHELL_PROCESSES_MIN))]
    pub max_shell_processes_per_run: usize,
    /// Lines a file read returns when the model names no limit.
    #[garde(range(min = 1))]
    pub default_file_read_lines: usize,
    /// Larger images are scaled down to this before the model sees them.
    #[garde(range(min = 1))]
    pub max_image_dimension_px: u32,
    /// Largest image sent to the model after scaling.
    #[garde(range(min = 1))]
    pub max_image_mib: u64,
    /// Longest a sleep may wait.
    #[garde(range(min = 1))]
    pub max_sleep_hours: u64,
    /// Longest a run waits for a subagent.
    #[garde(range(min = 1))]
    pub max_subagent_wait_seconds: u64,
    /// Subagents open at once in one run.
    #[garde(range(min = 1))]
    pub max_open_subagents: usize,
    /// How deep subagents may start their own subagents.
    #[garde(range(min = 1, max = SUBAGENT_DEPTH_MAX))]
    pub max_subagent_depth: u8,
    /// The most of one file a single read carries.
    #[garde(range(min = 1))]
    pub max_file_read_mib: u64,
    /// The most of a command's unread output kept at each end, its head and
    /// its tail.
    #[garde(range(min = 1))]
    pub output_edge_kib: usize,
}

/// Whole milliseconds in `duration`, which is far below `u64::MAX` of them.
fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl Default for ToolsTuning {
    fn default() -> Self {
        let run = afr_agent::RunLimits::DEFAULT;
        let tools = run.tools;
        Self {
            default_shell_timeout_seconds: tools.default_timeout.as_secs(),
            min_command_output_wait_ms: millis(tools.min_yield),
            max_command_output_wait_ms: millis(tools.max_yield),
            max_shell_processes_per_run: run.max_sessions,
            default_file_read_lines: tools.default_read_lines,
            max_image_dimension_px: tools.max_image_dimension,
            max_image_mib: tools.max_image_bytes / MIB,
            max_sleep_hours: tools.max_sleep.as_secs() / 3600,
            max_subagent_wait_seconds: tools.max_wait.as_secs(),
            max_open_subagents: run.caps.open,
            max_subagent_depth: run.caps.depth,
            max_file_read_mib: run.sizes.max_read_bytes / MIB,
            output_edge_kib: run.sizes.edge_bytes / KIB,
        }
    }
}

impl ToolsTuning {
    /// What the agent loop holds every run to, its executor holding to
    /// `sizes`.
    pub(super) fn run_limits(&self, sizes: afr_executor::Sizes) -> afr_agent::RunLimits {
        afr_agent::RunLimits {
            tools: guffbolt::tools::Limits {
                default_timeout: Duration::from_secs(self.default_shell_timeout_seconds),
                min_yield: Duration::from_millis(self.min_command_output_wait_ms),
                max_yield: Duration::from_millis(self.max_command_output_wait_ms),
                default_read_lines: self.default_file_read_lines,
                max_image_dimension: self.max_image_dimension_px,
                max_image_bytes: self.max_image_mib.saturating_mul(MIB),
                max_sleep: Duration::from_hours(self.max_sleep_hours),
                max_wait: Duration::from_secs(self.max_subagent_wait_seconds),
            },
            max_sessions: self.max_shell_processes_per_run,
            caps: guffbolt::agents::Caps {
                depth: self.max_subagent_depth,
                open: self.max_open_subagents,
            },
            sizes,
        }
    }
}

/// The link between the runner and each sandbox's executor.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ExecutorTuning {
    /// The longest message either end reads; a full directory listing must
    /// fit.
    #[garde(range(min = FRAME_MIB_MIN))]
    pub max_frame_mib: usize,
}

impl Default for ExecutorTuning {
    fn default() -> Self {
        Self {
            max_frame_mib: afr_executor::Sizes::DEFAULT.max_frame_bytes / MIB_USIZE,
        }
    }
}

/// Refuses a frame too short to carry one whole read of `read_mib`: base64
/// writes four bytes for every three, and the answer around it needs room.
pub(super) fn carries_a_read(read_mib: u64) -> impl FnOnce(&ExecutorTuning, &()) -> garde::Result {
    move |executor, ()| {
        let frame_mib = u64::try_from(executor.max_frame_mib).unwrap_or(u64::MAX);
        if frame_mib.saturating_mul(3) > read_mib.saturating_mul(4) {
            Ok(())
        } else {
            Err(garde::Error::new(
                "max_frame_mib must exceed four thirds of [tools] max_file_read_mib",
            ))
        }
    }
}

/// What every executor link holds to: the read size and output edges
/// `tools` chooses, and the frame `executor` chooses. The engine starts each
/// executor with them and the tools word their limits by them.
pub(super) const fn sizes(tools: &ToolsTuning, executor: ExecutorTuning) -> afr_executor::Sizes {
    afr_executor::Sizes {
        max_read_bytes: tools.max_file_read_mib.saturating_mul(MIB),
        edge_bytes: tools.output_edge_kib.saturating_mul(KIB),
        max_frame_bytes: executor.max_frame_mib.saturating_mul(MIB_USIZE),
    }
}

/// Calls to model providers.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ProviderTuning {
    /// How long connecting to a provider for a decision call may take.
    #[garde(range(min = 1))]
    pub decision_connect_timeout_seconds: u64,
    /// How long one decision call may take, its whole reply included.
    #[garde(range(min = 1))]
    pub decision_call_timeout_seconds: u64,
    /// How long a token-count call may take.
    #[garde(range(min = 1))]
    pub token_count_timeout_seconds: u64,
    /// Largest token-count reply accepted.
    #[garde(range(min = 1))]
    pub max_token_count_reply_kib: usize,
}

impl Default for ProviderTuning {
    fn default() -> Self {
        let decision = af_providers::decision::DecisionTimeouts::DEFAULT;
        let inference = af_providers::settings::Timeouts::DEFAULT;
        Self {
            decision_connect_timeout_seconds: decision.connect.as_secs(),
            decision_call_timeout_seconds: decision.physical.as_secs(),
            token_count_timeout_seconds: inference.count().as_secs(),
            max_token_count_reply_kib: inference.count_reply_bytes() / KIB,
        }
    }
}

impl ProviderTuning {
    /// How long a decision call may take to connect, and to finish.
    pub(super) const fn decision(&self) -> af_providers::decision::DecisionTimeouts {
        af_providers::decision::DecisionTimeouts {
            connect: Duration::from_secs(self.decision_connect_timeout_seconds),
            physical: Duration::from_secs(self.decision_call_timeout_seconds),
        }
    }

    /// `timeouts`, a native token count bounded as this section says.
    pub(super) const fn counting(
        &self,
        timeouts: af_providers::settings::Timeouts,
    ) -> af_providers::settings::Timeouts {
        timeouts.with_count(
            Duration::from_secs(self.token_count_timeout_seconds),
            self.max_token_count_reply_kib.saturating_mul(KIB),
        )
    }
}

/// Outbound HTTP from tools.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct EgressTuning {
    /// How long connecting may take; no longer than a whole request.
    #[garde(range(min = 1, max = self.request_timeout_seconds))]
    pub connect_timeout_seconds: u64,
    /// How long one request may take.
    #[garde(range(min = 1))]
    pub request_timeout_seconds: u64,
    /// Redirects followed before giving up.
    #[garde(range(min = 1))]
    pub max_redirects: u8,
    /// Memory the matcher that masks a redirect's secrets in its response may
    /// build; past it the response is refused.
    #[garde(range(min = 1))]
    pub max_secret_mask_mib: usize,
}

impl Default for EgressTuning {
    fn default() -> Self {
        let limits = af_egress::Limits::DEFAULT;
        Self {
            connect_timeout_seconds: limits.connect_timeout.as_secs(),
            request_timeout_seconds: limits.request_timeout.as_secs(),
            max_redirects: limits.max_redirects,
            max_secret_mask_mib: limits.mask_matcher_bytes / MIB_USIZE,
        }
    }
}

impl EgressTuning {
    /// The bounds every egress request is held to.
    pub(super) const fn limits(&self) -> af_egress::Limits {
        af_egress::Limits {
            connect_timeout: Duration::from_secs(self.connect_timeout_seconds),
            request_timeout: Duration::from_secs(self.request_timeout_seconds),
            max_redirects: self.max_redirects,
            mask_matcher_bytes: self.max_secret_mask_mib.saturating_mul(MIB_USIZE),
        }
    }
}

/// This runner's link to `agentsfleetd`.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct DaemonTuning {
    /// How long one call to the daemon may take; longer than the daemon's
    /// own message deadline.
    #[garde(range(min = afr_supervisor::config::CALL_TIMEOUT_MIN_SECONDS))]
    pub call_timeout_seconds: u64,
    /// Activity updates held while the daemon is busy.
    #[garde(range(min = ACTIVITY_BATCHES_MIN))]
    pub max_activity_batches_held: usize,
    /// Largest held activity batch.
    #[garde(range(min = 1))]
    pub max_activity_batch_kib: usize,
}

impl Default for DaemonTuning {
    fn default() -> Self {
        let tuning = afr_supervisor::config::Tuning::DEFAULT;
        Self {
            call_timeout_seconds: tuning.call_timeout.as_secs(),
            max_activity_batches_held: tuning.activity_batches_held,
            max_activity_batch_kib: tuning.activity_batch_bytes / KIB,
        }
    }
}

/// Traces.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct TelemetryTuning {
    /// Traces kept per run.
    #[garde(range(min = 1))]
    pub max_spans_per_run: u32,
}

impl Default for TelemetryTuning {
    fn default() -> Self {
        Self {
            max_spans_per_run: afr_telemetry::MAX_RUN_SPANS,
        }
    }
}

/// The supervisor's bounds: its daemon link, its decision calls and what
/// every sandbox enforces.
pub(super) fn supervisor(
    sandbox: &SandboxTuning,
    provider: &ProviderTuning,
    daemon: &DaemonTuning,
) -> afr_supervisor::config::Tuning {
    afr_supervisor::config::Tuning {
        call_timeout: Duration::from_secs(daemon.call_timeout_seconds),
        activity_batches_held: daemon.max_activity_batches_held,
        activity_batch_bytes: daemon.max_activity_batch_kib.saturating_mul(KIB),
        decision: provider.decision(),
        limits: sandbox.limits(),
    }
}
