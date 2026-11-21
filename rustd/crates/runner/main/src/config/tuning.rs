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

/// A default command timeout, with the executor's ending grace, stays under
/// the run's lease.
const SHELL_TIMEOUT_SECONDS_MAX: u64 = afr_tools::DEFAULT_TIMEOUT_CEILING.as_secs() - 1;

/// Room is never made from the most recently used sessions, so a cap at or
/// below their count would not hold.
const SESSIONS_MIN: usize = guffbolt::tools::process::sessions::SESSIONS_PROTECTED + 1;

/// A restore checks a checkpoint's children against this depth.
const SUBAGENT_DEPTH_MAX: u8 = afr_agent::RunLimits::DEFAULT.caps.depth;

/// One live-tail batch in flight and one queued.
const ACTIVITY_BATCHES_MIN: usize = 2;

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
        let limits = afr_sandbox::Limits::DEFAULT;
        let engine = afr_sandbox::EngineTuning::DEFAULT;
        Self {
            memory_mib: limits.memory_bytes / MIB,
            disk_mib: limits.disk_bytes / MIB,
            io_mib_per_second: engine.io_bytes_per_second / MIB,
            ready_timeout_seconds: engine.ready_timeout.as_secs(),
        }
    }
}

impl SandboxTuning {
    /// What every run's sandbox enforces.
    pub(super) fn limits(&self) -> afr_sandbox::Limits {
        afr_sandbox::Limits {
            memory_bytes: self.memory_mib.saturating_mul(MIB),
            disk_bytes: self.disk_mib.saturating_mul(MIB),
            ..afr_sandbox::Limits::DEFAULT
        }
    }

    /// How the engine starts and paces every sandbox.
    pub(super) fn engine(&self) -> afr_sandbox::EngineTuning {
        afr_sandbox::EngineTuning {
            ready_timeout: Duration::from_secs(self.ready_timeout_seconds),
            io_bytes_per_second: self.io_mib_per_second.saturating_mul(MIB),
        }
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
    #[garde(range(min = 1, max = self.max_exec_yield_ms))]
    pub min_exec_yield_ms: u64,
    /// Longest wait for a running command's output.
    #[garde(range(min = 1))]
    pub max_exec_yield_ms: u64,
    /// Interactive shell sessions one run may keep open.
    #[garde(range(min = SESSIONS_MIN))]
    pub max_sessions_per_run: usize,
    /// Lines a read returns when the model names no limit.
    #[garde(range(min = 1))]
    pub default_read_lines: usize,
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
    pub max_wait_seconds: u64,
    /// Subagents open at once in one run.
    #[garde(range(min = 1))]
    pub max_open_subagents: usize,
    /// How deep subagents may start their own subagents.
    #[garde(range(min = 1, max = SUBAGENT_DEPTH_MAX))]
    pub max_subagent_depth: u8,
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
            min_exec_yield_ms: millis(tools.min_yield),
            max_exec_yield_ms: millis(tools.max_yield),
            max_sessions_per_run: run.max_sessions,
            default_read_lines: tools.default_read_lines,
            max_image_dimension_px: tools.max_image_dimension,
            max_image_mib: tools.max_image_bytes / MIB,
            max_sleep_hours: tools.max_sleep.as_secs() / 3600,
            max_wait_seconds: tools.max_wait.as_secs(),
            max_open_subagents: run.caps.open,
            max_subagent_depth: run.caps.depth,
        }
    }
}

impl ToolsTuning {
    /// What the agent loop holds every run to.
    pub(super) fn run_limits(&self) -> afr_agent::RunLimits {
        afr_agent::RunLimits {
            tools: guffbolt::tools::Limits {
                default_timeout: Duration::from_secs(self.default_shell_timeout_seconds),
                min_yield: Duration::from_millis(self.min_exec_yield_ms),
                max_yield: Duration::from_millis(self.max_exec_yield_ms),
                default_read_lines: self.default_read_lines,
                max_image_dimension: self.max_image_dimension_px,
                max_image_bytes: self.max_image_mib.saturating_mul(MIB),
                max_sleep: Duration::from_hours(self.max_sleep_hours),
                max_wait: Duration::from_secs(self.max_wait_seconds),
            },
            max_sessions: self.max_sessions_per_run,
            caps: guffbolt::agents::Caps {
                depth: self.max_subagent_depth,
                open: self.max_open_subagents,
            },
        }
    }
}

/// Calls to model providers.
#[derive(Debug, Clone, Copy, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ProviderTuning {
    /// How long connecting to a provider for a decision call may take.
    #[garde(range(min = 1))]
    pub connect_timeout_seconds: u64,
    /// How long one decision call may take, its whole reply included.
    #[garde(range(min = 1))]
    pub physical_timeout_seconds: u64,
    /// How long a token-count call may take.
    #[garde(range(min = 1))]
    pub count_timeout_seconds: u64,
    /// Largest token-count reply accepted.
    #[garde(range(min = 1))]
    pub max_count_reply_kib: usize,
}

impl Default for ProviderTuning {
    fn default() -> Self {
        let decision = af_providers::decision::DecisionTimeouts::DEFAULT;
        let inference = af_inference::Timeouts::DEFAULT;
        Self {
            connect_timeout_seconds: decision.connect.as_secs(),
            physical_timeout_seconds: decision.physical.as_secs(),
            count_timeout_seconds: inference.count().as_secs(),
            max_count_reply_kib: inference.count_reply_bytes() / KIB,
        }
    }
}

impl ProviderTuning {
    /// How long a decision call may take to connect, and to finish.
    pub(super) const fn decision(&self) -> af_providers::decision::DecisionTimeouts {
        af_providers::decision::DecisionTimeouts {
            connect: Duration::from_secs(self.connect_timeout_seconds),
            physical: Duration::from_secs(self.physical_timeout_seconds),
        }
    }

    /// `timeouts`, a native token count bounded as this section says.
    pub(super) const fn counting(
        &self,
        timeouts: af_inference::Timeouts,
    ) -> af_inference::Timeouts {
        timeouts.with_count(
            Duration::from_secs(self.count_timeout_seconds),
            self.max_count_reply_kib.saturating_mul(KIB),
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
}

impl Default for EgressTuning {
    fn default() -> Self {
        let limits = af_egress::Limits::DEFAULT;
        Self {
            connect_timeout_seconds: limits.connect_timeout.as_secs(),
            request_timeout_seconds: limits.request_timeout.as_secs(),
            max_redirects: limits.max_redirects,
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
