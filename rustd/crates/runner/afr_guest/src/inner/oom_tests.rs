#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
)]

use std::fs;

use super::spare;

/// The executor's standing is the one the kernel never picks.
#[test]
fn the_executor_is_never_the_out_of_memory_killers_pick() {
    let standing = tempfile::NamedTempFile::new().unwrap();

    spare(standing.path()).unwrap();

    assert_eq!(fs::read_to_string(standing.path()).unwrap(), "-1000");
}
