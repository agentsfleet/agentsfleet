#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot write"
)]

use std::path::{Path, PathBuf};

use super::{HostTools, run, tail};

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
