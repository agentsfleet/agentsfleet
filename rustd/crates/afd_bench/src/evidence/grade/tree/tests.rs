//! Exact-tree validator regressions.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::fs;

use super::{Kind, require_entries};

#[test]
fn an_extra_entry_invalidates_an_immutable_directory() {
    let directory = std::env::temp_dir().join(format!(
        "afd-bench-tree-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("extra")
    ));
    fs::create_dir_all(&directory).expect("the test directory can be created");
    fs::write(directory.join("expected.json"), b"{}\n").expect("the expected file can be written");
    require_entries(&directory, vec![("expected.json".to_owned(), Kind::File)])
        .expect("the exact directory is accepted");

    fs::write(directory.join("extra.json"), b"{}\n").expect("the extra file can be written");
    let refusal = require_entries(&directory, vec![("expected.json".to_owned(), Kind::File)])
        .expect_err("an extra file must invalidate immutable evidence");

    assert!(refusal.to_string().contains("members are not exact"));
    fs::remove_dir_all(directory).expect("the test directory can be removed");
}
