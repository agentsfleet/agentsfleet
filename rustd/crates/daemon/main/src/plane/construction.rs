//! Construction groups retain each shared owner while the serving plane is assembled.

use super::{PlaneParts, durable_task_plane, run_plane};
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use af_observability::Analytics;
use afd_billing::biller::Biller;
use afd_credential::provider::Providers;
use afd_db::Db;
use afd_fleet::run::RunService;
use afd_runner::Runners;
use afd_turns::Turns;
use std::sync::Arc;

type StorageConfig =
    Option<garde::Valid<afd_fleet::durable_task::storage::CheckpointStorageConfig>>;

pub(super) struct Common {
    pub(super) providers: Providers,
    pub(super) github_client: reqwest::Client,
    pub(super) durable_task_master: Arc<Kek>,
    pub(super) turns: Turns,
    pub(super) biller: Arc<dyn Biller>,
    pub(super) creations: afd_setup::Creations,
}

impl Common {
    pub(super) fn new(parts: &PlaneParts) -> Self {
        let providers = Providers::new(
            parts.database.clone(),
            Arc::clone(&parts.kek),
            Entropy::new(),
        )
        .with_platform_key_ttl(crate::config::tuning::minutes(
            parts
                .runtime
                .operations
                .tuning
                .credentials
                .platform_key_cache_minutes,
        ));
        let github_client = parts.storage_client.clone();
        let durable_task_master = Arc::clone(&parts.kek);
        let tuning = parts.runtime.operations.tuning;
        let turns = Turns::new(parts.database.clone(), parts.bus.clone(), Entropy::new())
            .with_budgets(afd_turns::budget::Budgets {
                deployment_backlog: tuning.turns.max_queued_turns,
            })
            .with_notice_fanout(tuning.server.max_live_activity_notices);
        let biller = crate::billing::biller(parts.database.clone());
        let creations =
            afd_setup::Creations::new(parts.database.clone(), Entropy::new(), turns.clone())
                .with_tuning(setup_tuning(tuning.setup))
                .with_list_page(pages(&tuning.api).list);
        Self {
            providers,
            github_client,
            durable_task_master,
            turns,
            biller,
            creations,
        }
    }
}

pub(super) struct NativeInputs<'a> {
    pub(super) runtime: crate::preflight::RuntimeSettings,
    pub(super) database: &'a Db,
    pub(super) kek: Arc<Kek>,
    pub(super) providers: Providers,
    pub(super) bus: afd_bus::Handle,
    pub(super) broker: Arc<afd_credential::credential::Broker>,
    pub(super) biller: Arc<dyn Biller>,
    pub(super) wake: afd_bus::wake::Wake,
    pub(super) turns: Turns,
    pub(super) admin_workspace: Option<Uuid7>,
    pub(super) storage_config: StorageConfig,
    pub(super) master: Arc<Kek>,
    pub(super) client: reqwest::Client,
    pub(super) stream_limit: afd_sse::StreamLimit,
    pub(super) frame_memory: af_stream::MemoryLimits,
    pub(super) analytics: Analytics,
}

pub(super) struct Native {
    pub(super) runs: RunService,
    pub(super) tasks: afd_fleet::durable_task::DurableTasks,
    pub(super) runners: Runners,
    pub(super) streams: afd_gateway::Gateway,
}

impl NativeInputs<'_> {
    pub(super) fn build(self) -> Native {
        let mut runs = run_plane(
            self.database.clone(),
            self.kek,
            self.providers,
            self.bus.clone(),
            self.broker,
            self.biller,
            self.turns.notices(),
        );
        runs.runs = runs
            .runs
            .with_wake(self.wake.clone())
            .with_clock(self.runtime.clock)
            .with_checkpoint_resume(self.runtime.resume_from_checkpoint)
            .with_limits(run_limits(&self.runtime.operations.tuning));
        runs.thread = Arc::new(
            afd_fleet::run::history::Conversations::new(self.database.clone()).with_page_turns(
                i64::from(self.runtime.operations.tuning.turns.history_turns_per_page),
            ),
        );
        let tasks = durable_task_plane(
            &mut runs,
            self.turns,
            self.admin_workspace,
            self.storage_config,
            self.master,
            self.client,
            self.runtime.operations.tasks,
        );
        let runners =
            Runners::new(self.database.clone(), Entropy::new()).with_clock(self.runtime.clock);
        let streams = afd_gateway::Gateway::new(afd_gateway::GatewayDependencies {
            recovery: afd_gateway::RecoverySettings {
                max_concurrent_assignments: self.runtime.assignments,
                max_repair_backoff: std::time::Duration::from_secs(u64::from(
                    self.runtime
                        .operations
                        .tuning
                        .runners
                        .max_subscription_repair_backoff_seconds,
                )),
            },
            database: self.database.clone(),
            runners: runners.clone(),
            runs: runs.clone(),
            bus: self.bus,
            stream_limit: self.stream_limit,
            wake: self.wake,
            frame_memory: self.frame_memory,
            wall_clock: Arc::new(af_core::clock::SystemClock),
            analytics: self.analytics,
            frames: afd_gateway::FrameCeilings {
                message_bytes: mib_bytes(self.runtime.operations.tuning.runners.max_message_mib),
                control_message_bytes: mib_bytes(
                    self.runtime
                        .operations
                        .tuning
                        .runners
                        .max_control_message_mib,
                ),
            },
        });
        Native {
            runs,
            tasks,
            runners,
            streams,
        }
    }
}

/// Bytes in `mib` mebibytes; the configured ceilings sit far below 4 GiB.
const fn mib_bytes(mib: u32) -> u32 {
    mib.saturating_mul(1024 * 1024)
}

/// The run plane's settings from the deployment file's `[runners]`,
/// `[turns]`, `[billing]` and `[durable_tasks]` sections.
fn run_limits(tuning: &crate::config::tuning::Tuning) -> afd_fleet::run::RunLimits {
    let minutes = |value: u32| std::time::Duration::from_secs(u64::from(value) * 60);
    let days = |value: u32| af_core::timing::DAY.saturating_mul(value);
    let checkpoints = tuning.turns.checkpoints;
    let tasks = tuning.tasks;
    afd_fleet::run::RunLimits {
        max_runs_per_turn: tuning.turns.max_runs_per_turn,
        assign_batch_size: tuning.runners.turns_per_assignment_pass,
        price_book_cache_count: tuning.billing.price_book_cache_count,
        turn_checkpoints: afd_fleet::run::TurnCheckpointLimits {
            retention: days(checkpoints.retention_days),
            transfer_window: minutes(checkpoints.transfer_window_minutes),
            restore_pin: minutes(checkpoints.restore_pin_minutes),
            orphan_after: minutes(checkpoints.orphan_after_minutes),
        },
        tasks: afd_fleet::durable_task::policy::DurableTaskTimes {
            slice: std::time::Duration::from_secs(u64::from(tasks.slice_seconds)),
            operation_timeout: std::time::Duration::from_secs(u64::from(
                tasks.operation_timeout_seconds,
            )),
            terminal_retention: days(tasks.terminal_retention_days),
            keep_generations: tasks.checkpoints.keep_generations,
            upload_link_ttl: minutes(tasks.checkpoints.upload_link_ttl_minutes),
            download_link_ttl: minutes(tasks.checkpoints.download_link_ttl_minutes),
            transfer_window: minutes(tasks.checkpoints.transfer_window_minutes),
            restore_pin: minutes(tasks.checkpoints.restore_pin_minutes),
            orphan_after: minutes(tasks.checkpoints.orphan_after_minutes),
            delete_batch_size: tasks.checkpoints.objects_per_delete_call,
        },
        tool_windows: tool_windows(&tuning.context),
    }
}

/// The context tiers from the deployment file's `[context]`.
const fn tool_windows(
    context: &crate::config::tuning::ContextTuning,
) -> afd_gate::policy::context::ToolWindows {
    afd_gate::policy::context::ToolWindows {
        small_model_max_tokens: context.small_model_max_tokens,
        large_model_min_tokens: context.large_model_min_tokens,
        small: context.small_tool_window_count,
        default: context.default_tool_window_count,
        large: context.large_tool_window_count,
    }
}

/// The page sizes from the deployment file's `[api]`.
pub(super) const fn pages(api: &crate::config::tuning::ApiTuning) -> afd_http::paging::Pages {
    afd_http::paging::Pages {
        list: af_validate::Ceiling::new(api.max_page_size, api.default_page_size),
        thread: af_validate::Ceiling::new(api.max_thread_page_size, api.default_thread_page_size),
    }
}

/// A fleet create's settings from the deployment file's `[setup]`.
fn setup_tuning(setup: crate::config::tuning::SetupTuning) -> afd_setup::tuning::SetupTuning {
    afd_setup::tuning::SetupTuning {
        replay_window: std::time::Duration::from_secs(u64::from(setup.replay_window_hours) * 3_600),
    }
}

#[cfg(test)]
mod run_limit_tests {
    use super::run_limits;

    /// The compiled defaults of the file's sections are the run plane's own.
    #[test]
    fn the_file_defaults_are_the_run_planes_defaults() {
        let tuning = crate::config::tuning::Tuning::default();
        assert_eq!(run_limits(&tuning), afd_fleet::run::RunLimits::default());
    }

    /// A value the file sets is the value the run plane enforces.
    #[test]
    fn a_file_value_reaches_the_run_plane() {
        let mut tuning = crate::config::tuning::Tuning::default();
        tuning.turns.max_runs_per_turn = 5;
        tuning.runners.turns_per_assignment_pass = 9;
        tuning.billing.price_book_cache_count = 11;
        tuning.turns.checkpoints.restore_pin_minutes = 4;
        tuning.tasks.slice_seconds = 120;
        tuning.tasks.checkpoints.objects_per_delete_call = 16;
        let limits = run_limits(&tuning);
        assert_eq!(limits.max_runs_per_turn, 5);
        assert_eq!(limits.assign_batch_size, 9);
        assert_eq!(limits.price_book_cache_count, 11);
        assert_eq!(limits.turn_checkpoints.restore_pin.as_secs(), 240);
        assert_eq!(limits.tasks.slice.as_secs(), 120);
        assert_eq!(limits.tasks.delete_batch_size, 16);
    }
}
