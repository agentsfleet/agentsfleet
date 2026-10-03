#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::path::{Path, PathBuf};

use std::ffi::OsString;

use super::{HostTools, format_arguments, mount_arguments, run, tail};

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
fn test_tail_keeps_the_end_and_cuts_on_a_character() {
    let long = "é".repeat(3_000);

    let kept = tail(long.as_bytes());

    assert!(kept.len() <= 2_048 && kept.len() >= 2_046, "{}", kept.len());
    assert!(kept.chars().all(|letter| letter == 'é'));
    assert_eq!(tail(b"short\n\n"), "short");
}

#[test]
fn test_default_tools_are_where_debian_puts_them() {
    let tools = HostTools::default();

    assert_eq!(tools.mke2fs, Path::new(super::MKE2FS_PATH));
    assert_eq!(tools.mount, Path::new(super::MOUNT_PATH));
    assert_eq!(tools.bwrap, Path::new(crate::probe::BWRAP_PATH));
}

/// The owner a lease's workspace disk is formatted for.
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

#[test]
fn test_mount_arguments_name_type_options_source_and_target() {
    let arguments = strings(mount_arguments(
        "ext4",
        "loop,nosuid,nodev",
        Path::new("/a"),
        Path::new("/b"),
    ));

    assert_eq!(
        arguments,
        ["-t", "ext4", "-o", "loop,nosuid,nodev", "/a", "/b"]
    );
}
