//! Each section's values reach the runtime the plane is built from, and each
//! unsafe value refuses startup.

use std::io::Write as _;
use std::time::Duration;

use af_core::env::MapEnv;

use super::ConfigBuilder;
use super::tuning::Tuning;

type Checked = Result<(), Box<dyn std::error::Error>>;

fn written(text: &str) -> Result<tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(text.as_bytes())?;
    Ok(file)
}

fn configured<'a>(
    file: &'a tempfile::NamedTempFile,
    environment: &'a MapEnv,
) -> Result<super::Config<'a, MapEnv>, Box<dyn std::error::Error>> {
    Ok(ConfigBuilder::new(environment)
        .file(Some(file.path()))
        .build()?)
}

fn tuned(text: &str) -> Result<Tuning, Box<dyn std::error::Error>> {
    let (file, environment) = (written(text)?, MapEnv::default());
    Ok(configured(&file, &environment)?.runtime.operations.tuning)
}

#[test]
fn an_empty_file_selects_the_compiled_defaults() -> Checked {
    let tuning = tuned("")?;
    assert_eq!(format!("{tuning:?}"), format!("{:?}", Tuning::default()));
    Ok(())
}

#[test]
fn server_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[server]\ndrain_timeout_seconds=7\n\
         background_task_join_timeout_seconds=12\nmax_live_activity_notices=2",
    )?;
    assert_eq!(tuning.server.drain_timeout, Duration::from_secs(7));
    assert_eq!(tuning.server.task_join_timeout, Duration::from_secs(12));
    assert_eq!(tuning.server.max_live_activity_notices, 2);
    Ok(())
}

#[test]
fn api_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[api]\ndefault_page_size=10\nmax_page_size=40\n\
         default_thread_page_size=5\nmax_thread_page_size=30",
    )?;
    assert_eq!(
        (tuning.api.default_page_size, tuning.api.max_page_size),
        (10, 40)
    );
    assert_eq!(
        (
            tuning.api.default_thread_page_size,
            tuning.api.max_thread_page_size
        ),
        (5, 30)
    );
    Ok(())
}

/// The file's pool values are defaults: the deployment's environment knob
/// still wins over them.
#[test]
fn database_values_are_defaults_the_environment_overrides() -> Checked {
    let file = "[database]\nmax_connections=12\nmin_connections=3\n\
                acquire_timeout_seconds=7\nconnect_timeout_seconds=8\n\
                idle_timeout_minutes=4\nmax_connection_lifetime_minutes=9";
    let (file, environment) = (written(file)?, MapEnv::default());
    let pool = configured(&file, &environment)?.api_pool;
    assert_eq!((pool.max_connections, pool.min_connections), (12, 3));
    assert_eq!(pool.acquire_timeout_ms, 7_000);
    assert_eq!(pool.connect_timeout_ms, 8_000);
    assert_eq!(pool.idle_timeout_seconds, 240);
    assert_eq!(pool.max_lifetime_seconds, 540);
    let environment = MapEnv::from_pairs([("DATABASE_POOL_SIZE", "2")]);
    let pool = configured(&file, &environment)?.api_pool;
    assert_eq!((pool.max_connections, pool.min_connections), (2, 2));
    Ok(())
}

#[test]
fn nats_values_bound_the_wake_queue() -> Checked {
    let tuning = tuned("[nats]\ndefault_wake_queue_size=8\nmax_wake_queue_size=128")?;
    assert_eq!(
        tuning.wake,
        afd_bus::wake::QueueBounds {
            default: 8,
            max: 128
        }
    );
    Ok(())
}

#[test]
fn runners_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[runners]\nmax_message_mib=16\n\
         max_control_message_mib=1\nturns_per_assignment_pass=8\nmax_subscription_repair_backoff_seconds=2",
    )?;
    let runners = tuning.runners;
    assert_eq!(
        (runners.max_message_mib, runners.max_control_message_mib),
        (16, 1)
    );
    assert_eq!(
        (
            runners.turns_per_assignment_pass,
            runners.max_subscription_repair_backoff_seconds
        ),
        (8, 2)
    );
    Ok(())
}

#[test]
fn turns_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[turns]\nmax_runs_per_turn=5\nmax_queued_turns=10\nhistory_turns_per_page=7\n\
         [turns.checkpoints]\nretention_days=2\ntransfer_window_minutes=3\n\
         restore_pin_minutes=4\norphan_after_minutes=5",
    )?;
    let turns = tuning.turns;
    assert_eq!(
        (
            turns.max_runs_per_turn,
            turns.max_queued_turns,
            turns.history_turns_per_page
        ),
        (5, 10, 7)
    );
    let checkpoints = turns.checkpoints;
    assert_eq!(
        (
            checkpoints.retention_days,
            checkpoints.transfer_window_minutes,
            checkpoints.restore_pin_minutes,
            checkpoints.orphan_after_minutes
        ),
        (2, 3, 4, 5)
    );
    Ok(())
}

#[test]
fn context_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[context]\nsmall_model_max_tokens=1000\nlarge_model_min_tokens=2000\n\
         small_tool_window_count=1\ndefault_tool_window_count=2\nlarge_tool_window_count=3",
    )?;
    let context = tuning.context;
    assert_eq!(
        (
            context.small_model_max_tokens,
            context.large_model_min_tokens
        ),
        (1_000, 2_000)
    );
    assert_eq!(
        (
            context.small_tool_window_count,
            context.default_tool_window_count,
            context.large_tool_window_count
        ),
        (1, 2, 3)
    );
    Ok(())
}

#[test]
fn fleets_and_billing_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[fleets]\ndefinition_cache_mib=8\nhead_cache_count=9\n\
         [billing]\n\
         price_book_cache_count=7",
    )?;
    let fleets = tuning.fleets;
    assert_eq!(
        (fleets.definition_cache_mib, fleets.head_cache_count),
        (8, 9)
    );
    let billing = tuning.billing;
    assert_eq!(billing.price_book_cache_count, 7);
    Ok(())
}

#[test]
fn tasks_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[tasks]\nslice_seconds=60\noperation_timeout_seconds=6\nterminal_retention_days=1\n\
         [tasks.checkpoints]\nkeep_generations=2\nupload_link_ttl_minutes=3\n\
         download_link_ttl_minutes=4\ntransfer_window_minutes=8\nrestore_pin_minutes=5\n\
         orphan_after_minutes=6\nobjects_per_delete_call=7",
    )?;
    let tasks = tuning.tasks;
    assert_eq!(
        (
            tasks.slice_seconds,
            tasks.operation_timeout_seconds,
            tasks.terminal_retention_days
        ),
        (60, 6, 1)
    );
    let checkpoints = tasks.checkpoints;
    assert_eq!(
        (
            checkpoints.keep_generations,
            checkpoints.upload_link_ttl_minutes,
            checkpoints.download_link_ttl_minutes,
            checkpoints.transfer_window_minutes,
        ),
        (2, 3, 4, 8)
    );
    assert_eq!(
        (
            checkpoints.restore_pin_minutes,
            checkpoints.orphan_after_minutes,
            checkpoints.objects_per_delete_call
        ),
        (5, 6, 7)
    );
    Ok(())
}

#[test]
fn setup_values_reach_the_runtime() -> Checked {
    let setup = tuned(
        "[setup]\nrequest_timeout_seconds=9\nreplay_window_hours=2\nmaintenance_drafts_per_page=8",
    )?
    .setup;
    assert_eq!(
        (
            setup.request_timeout_seconds,
            setup.replay_window_hours,
            setup.maintenance_drafts_per_page
        ),
        (9, 2, 8)
    );
    Ok(())
}

#[test]
fn auth_values_reach_the_runtime() -> Checked {
    let auth = tuned(
        "[auth]\ncli_login_ttl_seconds=120\ncli_login_code_replay_seconds=30\n\
         max_cli_login_code_attempts=3\noauth_state_ttl_minutes=4\nidentity_timeout_seconds=6\n\
         jwks_fetch_timeout_seconds=7\njwks_cache_hours=2\n\
         jwks_stale_grace_minutes=9\njwks_min_refresh_seconds=11",
    )?
    .auth;
    assert_eq!(
        (
            auth.cli_login_ttl_seconds,
            auth.cli_login_code_replay_seconds,
            auth.max_cli_login_code_attempts
        ),
        (120, 30, 3)
    );
    assert_eq!(auth.oauth_state_ttl_minutes, 4);
    assert_eq!(
        (
            auth.identity_timeout_seconds,
            auth.jwks_fetch_timeout_seconds
        ),
        (6, 7)
    );
    assert_eq!(
        (
            auth.jwks_cache_hours,
            auth.jwks_stale_grace_minutes,
            auth.jwks_min_refresh_seconds
        ),
        (2, 9, 11)
    );
    Ok(())
}

#[test]
fn credentials_and_connector_values_reach_the_runtime() -> Checked {
    let tuning = tuned(
        "[credentials]\ntoken_cache_count=10\nplatform_key_cache_minutes=2\n\
         assumed_token_ttl_minutes=3\n\
         [connectors.github]\ninstallation_token_ttl_minutes=30\n\
         [connectors.slack]\npost_timeout_seconds=2\nanswer_check_timeout_seconds=1\n\
         thread_read_timeout_ms=700\n\
         [connectors.mail]\nsend_timeout_seconds=4",
    )?;
    assert_eq!(
        (
            tuning.credentials.token_cache_count,
            tuning.credentials.platform_key_cache_minutes,
            tuning.credentials.assumed_token_ttl_minutes
        ),
        (10, 2, 3)
    );
    assert_eq!(
        tuning.minting().lifetimes.assumed_access,
        Duration::from_mins(3)
    );
    assert_eq!(tuning.github.installation_token_ttl_minutes, 30);
    let slack = tuning.slack;
    assert_eq!(
        (
            slack.post_timeout_seconds,
            slack.answer_check_timeout_seconds,
            slack.thread_read_timeout_ms
        ),
        (2, 1, 700)
    );
    assert_eq!(tuning.mail.send_timeout_seconds, 4);
    Ok(())
}

#[test]
fn qstash_values_reach_the_scheduler_retry_policy() -> Checked {
    let file = written(
        "[connectors.qstash]\nattempt_timeout_ms=900\ncall_budget_seconds=5\n\
         max_attempts=2\nmax_retry_delay_ms=300",
    )?;
    let environment = MapEnv::default();
    let config = configured(&file, &environment)?;
    assert_eq!(
        Some(config.runtime.operations.scheduler_retries),
        afd_cron::qstash::RetryPolicy::new(
            2,
            Duration::from_millis(900),
            Duration::from_millis(300),
            Duration::from_secs(5),
        )
    );
    let empty = written("")?;
    let defaults = configured(&empty, &environment)?;
    assert_eq!(
        defaults.runtime.operations.scheduler_retries,
        afd_cron::qstash::RetryPolicy::DEFAULT
    );
    Ok(())
}

#[test]
fn telemetry_values_reach_the_runtime() -> Checked {
    let metrics = tuned("[telemetry]\nmax_metric_series=10\nmetric_max_age_seconds=30")?.metrics;
    assert_eq!(metrics.max_metric_series, 10);
    assert_eq!(metrics.metric_max_age, Duration::from_secs(30));
    Ok(())
}

/// Each line refuses startup, naming the key it refuses.
const REFUSED: &[(&str, &str)] = &[
    ("[server]\ndrain_timeout_seconds=0", "drain_timeout_seconds"),
    (
        "[server]\nbackground_task_join_timeout_seconds=0",
        "background_task_join_timeout_seconds",
    ),
    (
        "[server]\nmax_live_activity_notices=0",
        "max_live_activity_notices",
    ),
    ("[api]\nmax_page_size=0", "max_page_size"),
    ("[api]\ndefault_page_size=101", "default_page_size"),
    (
        "[api]\nmax_thread_page_size=10\ndefault_thread_page_size=11",
        "default_thread_page_size",
    ),
    ("[database]\nmax_connections=0", "max_connections"),
    (
        "[database]\nmax_connections=4\nmin_connections=5",
        "min_connections",
    ),
    (
        "[database]\nacquire_timeout_seconds=0",
        "acquire_timeout_seconds",
    ),
    (
        "[database]\nconnect_timeout_seconds=0",
        "connect_timeout_seconds",
    ),
    ("[database]\nidle_timeout_minutes=0", "idle_timeout_minutes"),
    (
        "[database]\nmax_connection_lifetime_minutes=0",
        "max_connection_lifetime_minutes",
    ),
    ("[nats]\nmax_wake_queue_size=32", "default_wake_queue_size"),
    ("[nats]\nmax_wake_queue_size=0", "max_wake_queue_size"),
    (
        "[nats]\nmax_wake_queue_size=100\nwake_queue_capacity_fleets=101",
        "wake_queue_capacity_fleets",
    ),
    ("[runners]\nmax_message_mib=0", "max_message_mib"),
    ("[runners]\nmax_message_mib=33", "max_message_mib"),
    (
        "[runners]\nmax_control_message_mib=3",
        "max_control_message_mib",
    ),
    (
        "[runners]\nturns_per_assignment_pass=0",
        "turns_per_assignment_pass",
    ),
    (
        "[runners]\nmax_subscription_repair_backoff_seconds=0",
        "max_subscription_repair_backoff_seconds",
    ),
    ("[turns]\nmax_runs_per_turn=0", "max_runs_per_turn"),
    ("[turns]\nmax_queued_turns=0", "max_queued_turns"),
    (
        "[turns]\nhistory_turns_per_page=0",
        "history_turns_per_page",
    ),
    ("[turns.checkpoints]\nretention_days=0", "retention_days"),
    (
        "[turns.checkpoints]\ntransfer_window_minutes=0",
        "transfer_window_minutes",
    ),
    (
        "[turns.checkpoints]\nrestore_pin_minutes=0",
        "restore_pin_minutes",
    ),
    (
        "[turns.checkpoints]\norphan_after_minutes=0",
        "orphan_after_minutes",
    ),
    (
        "[context]\nsmall_model_max_tokens=1000000",
        "small_model_max_tokens",
    ),
    (
        "[context]\nlarge_model_min_tokens=0",
        "large_model_min_tokens",
    ),
    (
        "[context]\nsmall_tool_window_count=0",
        "small_tool_window_count",
    ),
    (
        "[context]\ndefault_tool_window_count=0",
        "default_tool_window_count",
    ),
    (
        "[context]\nlarge_tool_window_count=0",
        "large_tool_window_count",
    ),
    ("[fleets]\ndefinition_cache_mib=0", "definition_cache_mib"),
    ("[fleets]\nhead_cache_count=0", "head_cache_count"),
    (
        "[billing]\nprice_book_cache_count=0",
        "price_book_cache_count",
    ),
    ("[tasks]\nslice_seconds=0", "slice_seconds"),
    (
        "[tasks]\noperation_timeout_seconds=0",
        "operation_timeout_seconds",
    ),
    (
        "[tasks]\nterminal_retention_days=0",
        "terminal_retention_days",
    ),
    (
        "[tasks.checkpoints]\nkeep_generations=0",
        "keep_generations",
    ),
    (
        "[tasks.checkpoints]\nupload_link_ttl_minutes=0",
        "upload_link_ttl_minutes",
    ),
    (
        "[tasks.checkpoints]\ndownload_link_ttl_minutes=0",
        "download_link_ttl_minutes",
    ),
    (
        "[tasks.checkpoints]\ntransfer_window_minutes=0",
        "transfer_window_minutes",
    ),
    (
        "[tasks.checkpoints]\nobjects_per_delete_call=0",
        "objects_per_delete_call",
    ),
    (
        "[setup]\nrequest_timeout_seconds=0",
        "request_timeout_seconds",
    ),
    ("[setup]\nreplay_window_hours=0", "replay_window_hours"),
    (
        "[setup]\nmaintenance_drafts_per_page=0",
        "maintenance_drafts_per_page",
    ),
    ("[auth]\ncli_login_ttl_seconds=0", "cli_login_ttl_seconds"),
    (
        "[auth]\ncli_login_ttl_seconds=86401",
        "cli_login_ttl_seconds",
    ),
    (
        "[auth]\ncli_login_code_replay_seconds=86401",
        "cli_login_code_replay_seconds",
    ),
    (
        "[auth]\nmax_cli_login_code_attempts=0",
        "max_cli_login_code_attempts",
    ),
    (
        "[auth]\nmax_cli_login_code_attempts=11",
        "max_cli_login_code_attempts",
    ),
    (
        "[auth]\noauth_state_ttl_minutes=1441",
        "oauth_state_ttl_minutes",
    ),
    (
        "[auth]\nidentity_timeout_seconds=61",
        "identity_timeout_seconds",
    ),
    (
        "[auth]\njwks_fetch_timeout_seconds=0",
        "jwks_fetch_timeout_seconds",
    ),
    ("[auth]\njwks_cache_hours=25", "jwks_cache_hours"),
    (
        "[auth]\njwks_stale_grace_minutes=1441",
        "jwks_stale_grace_minutes",
    ),
    (
        "[auth]\njwks_min_refresh_seconds=0",
        "jwks_min_refresh_seconds",
    ),
    ("[credentials]\ntoken_cache_count=0", "token_cache_count"),
    (
        "[credentials]\nplatform_key_cache_minutes=0",
        "platform_key_cache_minutes",
    ),
    (
        "[credentials]\nassumed_token_ttl_minutes=0",
        "assumed_token_ttl_minutes",
    ),
    (
        "[credentials]\nassumed_token_ttl_minutes=1441",
        "assumed_token_ttl_minutes",
    ),
    (
        "[connectors.github]\ninstallation_token_ttl_minutes=61",
        "installation_token_ttl_minutes",
    ),
    (
        "[connectors.github]\ninstallation_token_ttl_minutes=0",
        "installation_token_ttl_minutes",
    ),
    (
        "[connectors.slack]\npost_timeout_seconds=0",
        "post_timeout_seconds",
    ),
    (
        "[connectors.slack]\nanswer_check_timeout_seconds=0",
        "answer_check_timeout_seconds",
    ),
    (
        "[connectors.slack]\nthread_read_timeout_ms=0",
        "thread_read_timeout_ms",
    ),
    (
        "[connectors.mail]\nsend_timeout_seconds=0",
        "send_timeout_seconds",
    ),
    (
        "[connectors.qstash]\nattempt_timeout_ms=0",
        "attempt_timeout_ms",
    ),
    (
        "[connectors.qstash]\ncall_budget_seconds=0",
        "call_budget_seconds",
    ),
    (
        "[connectors.qstash]\ncall_budget_seconds=1",
        "call_budget_seconds",
    ),
    ("[connectors.qstash]\nmax_attempts=0", "max_attempts"),
    (
        "[connectors.qstash]\nmax_retry_delay_ms=0",
        "max_retry_delay_ms",
    ),
    ("[telemetry]\nmax_metric_series=0", "max_metric_series"),
    (
        "[telemetry]\nmetric_max_age_seconds=0",
        "metric_max_age_seconds",
    ),
];

#[test]
fn each_unsafe_value_refuses_startup_naming_its_key() -> Checked {
    for &(text, key) in REFUSED {
        let (file, environment) = (written(text)?, MapEnv::default());
        let refused = configured(&file, &environment)
            .err()
            .ok_or_else(|| format!("{text:?} was accepted"))?;
        let details = std::iter::successors(Some(&*refused), |cause| cause.source())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            details.contains(key),
            "{text:?} refused without naming {key}: {details}"
        );
    }
    Ok(())
}
