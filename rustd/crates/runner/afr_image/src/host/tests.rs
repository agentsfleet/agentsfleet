#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::{HostTools, format_arguments, run, tail};

/// A program every Unix carries, by its absolute path.
fn shell() -> PathBuf {
    PathBuf::from("/bin/sh")
}

#[tokio::test]
async fn test_a_program_that_succeeds_runs_quietly() {
    run("sh", &shell(), ["-c", "echo out; exit 0"])
        .await
        .unwrap();
}

#[tokio::test]
async fn test_a_program_that_fails_is_reported_with_its_error_stream() {
    let failed = run(
        "sh",
        &shell(),
        ["-c", "echo first >&2; echo why it failed >&2; exit 3"],
    )
    .await
    .unwrap_err();

    let text = failed.to_string();
    assert!(text.contains("sh exited with"), "{text}");
    assert!(text.contains("why it failed"), "{text}");
}

#[tokio::test]
async fn test_a_program_that_is_not_there_is_an_input_output_failure() {
    let missing = run("nothing", Path::new("/nonexistent/nothing"), ["x"])
        .await
        .unwrap_err();

    assert!(missing.to_string().contains("input/output"), "{missing}");
}

#[test]
fn test_tail_keeps_the_whole_stream_without_trailing_space() {
    let long = "é".repeat(3_000);

    assert_eq!(tail(long.as_bytes()), long);
    assert_eq!(tail(b"short\n\n"), "short");
}

#[test]
fn test_default_tools_are_where_debian_puts_them() {
    let tools = HostTools::default();

    assert_eq!(tools.mke2fs, Path::new(super::MKE2FS_PATH));
    assert_eq!(tools.bwrap, Path::new(crate::host::BWRAP_PATH));
}

/// The owner a run's workspace disk is formatted for.
const USER: u32 = 1000;
/// Its group.
const GROUP: u32 = 100;

fn strings(arguments: Vec<OsString>) -> Vec<String> {
    arguments
        .into_iter()
        .map(|part| part.into_string().unwrap_or_default())
        .collect()
}

#[test]
fn test_format_arguments_make_a_journal_free_disk_its_owner_may_write() {
    let arguments = strings(format_arguments(
        Path::new("/s/workspace.img"),
        (USER, GROUP),
    ));

    assert_eq!(
        arguments,
        [
            "-q",
            "-F",
            "-b",
            "4096",
            "-t",
            "ext4",
            "-m",
            "0",
            "-O",
            "^has_journal",
            "-E",
            // pin test: literal is the contract
            "root_owner=1000:100",
            "/s/workspace.img",
        ]
    );
}
