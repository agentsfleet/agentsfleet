//! Each section's values reach the component built from them, and each
//! unsafe value, or a key the runner does not read, refuses startup.

use std::io::Write as _;
use std::time::Duration;

use af_core::env::MapEnv;
use afr_executor::Executor as _;

use super::{ENV_CONFIG_FILE, Settings, load, read};

type Checked = Result<(), Box<dyn std::error::Error>>;

/// Reads `text` as the selected profile.
fn settings(text: &str) -> Result<Settings, Box<dyn std::error::Error>> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(text.as_bytes())?;
    let path = file.path().to_str().ok_or("temp path is not UTF-8")?;
    read(&MapEnv::from_pairs([(ENV_CONFIG_FILE, path)]))
        .map_err(|status| format!("profile refused: {status:?}").into())
}

/// Whether `text` refuses loading, naming `field`.
fn refused_naming(text: &str, field: &str) -> Checked {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(text.as_bytes())?;
    let path = file.path().to_str().ok_or("temp path is not UTF-8")?;
    let env = MapEnv::from_pairs([(ENV_CONFIG_FILE, path)]);
    let failure = load(&env)
        .err()
        .ok_or_else(|| format!("{text} must refuse"))?;
    let details = std::iter::successors(Some(&failure as &dyn std::error::Error), |cause| {
        cause.source()
    })
    .map(ToString::to_string)
    .collect::<Vec<_>>()
    .join("\n");
    assert!(
        details.contains(field),
        "{text}: expected {field} in {details}"
    );
    assert!(read(&env).is_err(), "{text} must refuse startup");
    Ok(())
}

#[test]
fn an_empty_profile_selects_todays_values() -> Checked {
    let selected = settings("")?;
    assert_eq!(selected.tuning, afr_supervisor::config::Tuning::DEFAULT);
    assert_eq!(
        selected.firecracker.ready_timeout,
        afr_engine::DEFAULT_READY_TIMEOUT
    );
    assert_eq!(
        selected.firecracker.io_bytes_per_second,
        afr_cgroup::DEFAULT_IO_BYTES_PER_SECOND
    );
    assert_eq!(selected.run_limits, afr_agent::RunLimits::DEFAULT);
    assert_eq!(selected.egress, af_egress::Limits::DEFAULT);
    assert_eq!(selected.max_run_spans, afr_telemetry::MAX_RUN_SPANS);
    assert_eq!(
        selected.inference,
        af_providers::settings::Timeouts::DEFAULT
    );
    assert_eq!(selected.sizes, afr_executor::Sizes::DEFAULT);
    Ok(())
}

#[test]
fn sandbox_values_reach_the_limits_and_the_engine() -> Checked {
    let selected = settings(
        "[sandbox]\nmemory_mib = 512\ndisk_mib = 1_024\nio_mib_per_second = 50\n\
         ready_timeout_seconds = 4\n",
    )?;
    assert_eq!(selected.tuning.limits.memory_bytes, 512 * 1024 * 1024);
    assert_eq!(selected.tuning.limits.disk_bytes, 1024 * 1024 * 1024);
    assert_eq!(selected.firecracker.io_bytes_per_second, 50 * 1024 * 1024);
    assert_eq!(selected.firecracker.ready_timeout, Duration::from_secs(4));
    Ok(())
}

#[test]
fn tools_values_reach_the_run_limits() -> Checked {
    let selected = settings(
        "[tools]\ndefault_shell_timeout_seconds = 20\nmin_command_output_wait_ms = 100\n\
         max_command_output_wait_ms = 5_000\nmax_shell_processes_per_run = 16\ndefault_file_read_lines = 500\n\
         max_image_dimension_px = 1_024\nmax_image_mib = 2\nmax_sleep_hours = 1\n\
         max_subagent_wait_seconds = 60\nmax_open_subagents = 3\nmax_subagent_depth = 1\n",
    )?;
    let limits = selected.run_limits;
    assert_eq!(limits.tools.default_timeout, Duration::from_secs(20));
    assert_eq!(limits.tools.min_yield, Duration::from_millis(100));
    assert_eq!(limits.tools.max_yield, Duration::from_secs(5));
    assert_eq!(limits.max_sessions, 16);
    assert_eq!(limits.tools.default_read_lines, 500);
    assert_eq!(limits.tools.max_image_dimension, 1024);
    assert_eq!(limits.tools.max_image_bytes, 2 * 1024 * 1024);
    assert_eq!(limits.tools.max_sleep, Duration::from_hours(1));
    assert_eq!(limits.tools.max_wait, Duration::from_mins(1));
    assert_eq!((limits.caps.open, limits.caps.depth), (3, 1));
    Ok(())
}

#[test]
fn executor_sizes_reach_the_engine_and_the_tools() -> Checked {
    let selected = settings(
        "[tools]\nmax_file_read_mib = 2\noutput_edge_kib = 64\n\n\
         [executor]\nmax_frame_mib = 16\n",
    )?;
    let sizes = afr_executor::Sizes {
        max_read_bytes: 2 * 1024 * 1024,
        edge_bytes: 64 * 1024,
        max_frame_bytes: 16 * 1024 * 1024,
    };
    // The engine starts every executor with what the tools word their
    // limits by.
    assert_eq!(selected.sizes, sizes);
    assert_eq!(selected.firecracker.sizes, sizes);
    assert_eq!(selected.run_limits.sizes, sizes);
    Ok(())
}

/// A runner told to read 1 MiB at a time starts its executor so: the
/// executor cuts a 2 MiB file at 1 MiB, though the call asks for all of it.
#[tokio::test]
async fn the_executor_uses_the_sizes_the_runner_sent() -> Checked {
    let selected = settings("[tools]\nmax_file_read_mib = 1\n")?;
    let sizes = selected.firecracker.sizes;
    // What the engine tells the guest entry, as the entry reads it back.
    let told = afr_engine_firecracker::guest::GuestEntry::new(1, sizes).arguments();
    let argv = std::iter::once("agentsfleet-runner".into()).chain(told);
    let crate::Command::Guest(entry) = <crate::Cli as clap::Parser>::try_parse_from(argv)?.command
    else {
        return Err("the guest entry parses as itself".into());
    };
    let scratch = tempfile::tempdir()?;
    let (socket, root) = (
        scratch.path().join("executor.sock"),
        scratch.path().join("w"),
    );
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("big.bin"), vec![b'x'; 2 * 1024 * 1024])?;
    let server = afr_executor::bind(&socket)?.with_sizes(entry.sizes());
    let serving = tokio::spawn(async move { server.serve(&root).await });
    let client =
        afr_executor::Client::connect_within(&socket, Duration::from_secs(5), sizes).await?;

    let read = client.read_file("big.bin", u64::MAX).await?;

    assert_eq!(
        read.data.len(),
        1024 * 1024,
        "cut at the size the runner sent"
    );
    assert!(read.truncated, "the file runs past one read");
    drop(client);
    serving.await??;
    Ok(())
}

#[test]
fn provider_values_reach_decisions_and_counting() -> Checked {
    let selected = settings(
        "[provider]\ndecision_connect_timeout_seconds = 2\ndecision_call_timeout_seconds = 30\n\
         token_count_timeout_seconds = 4\nmax_token_count_reply_kib = 8\n",
    )?;
    assert_eq!(selected.tuning.decision.connect, Duration::from_secs(2));
    assert_eq!(selected.tuning.decision.physical, Duration::from_secs(30));
    assert_eq!(selected.inference.count(), Duration::from_secs(4));
    assert_eq!(selected.inference.count_reply_bytes(), 8 * 1024);
    // `[inference]` keeps its own keys beside `[provider]`'s.
    assert_eq!(
        selected.inference.connect(),
        af_providers::settings::Timeouts::DEFAULT.connect()
    );
    Ok(())
}

#[test]
fn egress_values_reach_the_network_limits() -> Checked {
    let selected = settings(
        "[egress]\nconnect_timeout_seconds = 5\nrequest_timeout_seconds = 60\nmax_redirects = 1\n\
         max_secret_mask_mib = 4\n",
    )?;
    assert_eq!(selected.egress.connect_timeout, Duration::from_secs(5));
    assert_eq!(selected.egress.request_timeout, Duration::from_mins(1));
    assert_eq!(selected.egress.max_redirects, 1);
    assert_eq!(selected.egress.mask_matcher_bytes, 4 * 1024 * 1024);
    Ok(())
}

#[test]
fn daemon_values_reach_the_supervisor() -> Checked {
    let selected = settings(
        "[daemon]\ncall_timeout_seconds = 40\nmax_activity_batches_held = 8\n\
         max_activity_batch_kib = 32\n",
    )?;
    assert_eq!(selected.tuning.call_timeout, Duration::from_secs(40));
    assert_eq!(selected.tuning.activity_batches_held, 8);
    assert_eq!(selected.tuning.activity_batch_bytes, 32 * 1024);
    Ok(())
}

#[test]
fn telemetry_values_reach_the_span_budget() -> Checked {
    assert_eq!(
        settings("[telemetry]\nmax_spans_per_run = 64\n")?.max_run_spans,
        64
    );
    Ok(())
}

/// Every count, size and timeout refuses zero.
const ZEROES: &[(&str, &str)] = &[
    ("sandbox", "memory_mib"),
    ("sandbox", "disk_mib"),
    ("sandbox", "io_mib_per_second"),
    ("sandbox", "ready_timeout_seconds"),
    ("tools", "default_shell_timeout_seconds"),
    ("tools", "min_command_output_wait_ms"),
    ("tools", "max_command_output_wait_ms"),
    ("tools", "max_shell_processes_per_run"),
    ("tools", "default_file_read_lines"),
    ("tools", "max_image_dimension_px"),
    ("tools", "max_image_mib"),
    ("tools", "max_sleep_hours"),
    ("tools", "max_subagent_wait_seconds"),
    ("tools", "max_open_subagents"),
    ("tools", "max_subagent_depth"),
    ("tools", "max_file_read_mib"),
    ("tools", "output_edge_kib"),
    ("executor", "max_frame_mib"),
    ("provider", "decision_connect_timeout_seconds"),
    ("provider", "decision_call_timeout_seconds"),
    ("provider", "token_count_timeout_seconds"),
    ("provider", "max_token_count_reply_kib"),
    ("egress", "connect_timeout_seconds"),
    ("egress", "request_timeout_seconds"),
    ("egress", "max_redirects"),
    ("egress", "max_secret_mask_mib"),
    ("daemon", "call_timeout_seconds"),
    ("daemon", "max_activity_batches_held"),
    ("daemon", "max_activity_batch_kib"),
    ("telemetry", "max_spans_per_run"),
];

#[test]
fn every_zero_count_size_and_timeout_is_refused() -> Checked {
    for (section, key) in ZEROES {
        refused_naming(&format!("[{section}]\n{key} = 0\n"), key)?;
    }
    Ok(())
}

#[test]
fn values_past_their_bounds_are_refused() -> Checked {
    // The shortest output wait may not exceed the longest.
    refused_naming(
        "[tools]\nmin_command_output_wait_ms = 2_000\nmax_command_output_wait_ms = 1_000\n",
        "min_command_output_wait_ms",
    )?;
    // A default command timeout past tokio's longest timer.
    refused_naming(
        "[tools]\ndefault_shell_timeout_seconds = 2_147_484\n",
        "default_shell_timeout_seconds",
    )?;
    // A shell-process cap the most recently used, never pruned, would overrun.
    refused_naming(
        "[tools]\nmax_shell_processes_per_run = 8\n",
        "max_shell_processes_per_run",
    )?;
    // Deeper than a restore checks a checkpoint's children against.
    refused_naming("[tools]\nmax_subagent_depth = 3\n", "max_subagent_depth")?;
    // Connecting may not outlast the whole request.
    refused_naming(
        "[egress]\nconnect_timeout_seconds = 40\nrequest_timeout_seconds = 30\n",
        "connect_timeout_seconds",
    )?;
    // A daemon call timing out before the daemon's own message deadline.
    refused_naming(
        "[daemon]\ncall_timeout_seconds = 12\n",
        "call_timeout_seconds",
    )?;
    // One batch in flight leaves none to queue.
    refused_naming(
        "[daemon]\nmax_activity_batches_held = 1\n",
        "max_activity_batches_held",
    )?;
    // A frame a full directory listing would not fit in.
    refused_naming("[executor]\nmax_frame_mib = 6\n", "max_frame_mib")?;
    // A frame too short to carry one whole read as base64.
    refused_naming(
        "[tools]\nmax_file_read_mib = 12\n\n[executor]\nmax_frame_mib = 16\n",
        "max_frame_mib",
    )?;
    Ok(())
}

/// The approved keys no constant stands behind, or whose constant the runner
/// cannot honour alone (`docs/architecture/config_reference.md`).
const NOT_READ: &[(&str, &str)] = &[
    ("sandbox", "server_grace_seconds"),
    ("sandbox", "far_connect_timeout_seconds"),
    ("sandbox", "max_attach_attempts"),
    ("sandbox", "min_retry_delay_ms"),
    ("sandbox", "max_retry_delay_seconds"),
    ("tools", "mcp_call_timeout_seconds"),
    ("egress", "max_response_mib"),
    ("egress", "max_construction_mib"),
    ("egress", "max_target_kib"),
    ("daemon", "max_report_mib"),
    ("daemon", "max_activity_kib"),
];

#[test]
fn keys_the_runner_does_not_read_are_refused() -> Checked {
    for (section, key) in NOT_READ {
        refused_naming(&format!("[{section}]\n{key} = 1\n"), key)?;
    }
    Ok(())
}
