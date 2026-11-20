//! The one gate every file tool passes its path through.

use super::*;

/// Every file tool passes its path through one gate: a path that names no
/// file, climbs out through `..` or is absolute outside the root is refused
/// before any read or write.
#[tokio::test]
async fn a_path_that_leaves_the_working_directory_is_refused_before_any_write() {
    let desk = Desk::new();
    let tools = desk.tools(false);

    let empty = call(&tools, "file_write", json!({"path": "", "content": "x"})).await;
    let here = call(&tools, "file_write", json!({"path": ".", "content": "x"})).await;
    let climbs = call(
        &tools,
        "file_write",
        json!({"path": "a/../../x", "content": "x"}),
    )
    .await;
    let absolute = call(
        &tools,
        "file_append",
        json!({"path": "/etc/hosts", "content": "x"}),
    )
    .await;
    let read = call(&tools, "file_read", json!({"path": "../x"})).await;

    let no_file = "[invalid_arguments] path must name a file in the working directory";
    assert_eq!(empty.text, no_file);
    assert_eq!(here.text, no_file);
    assert_eq!(
        climbs.text,
        format!("[path_not_allowed] a/../../x {LEAVES}")
    );
    assert_eq!(
        absolute.text,
        format!("[path_not_allowed] /etc/hosts {LEAVES}")
    );
    assert_eq!(read.text, format!("[path_not_allowed] ../x {LEAVES}"));
    let written: Vec<_> = std::fs::read_dir(desk.directory.path())
        .expect("listed")
        .collect();
    assert_eq!(written.len(), 0, "nothing was written");
}

/// An absolute path under the root names the file below it.
#[tokio::test]
async fn an_absolute_path_under_the_root_names_the_file_below_it() {
    let desk = Desk::new();
    let tools = desk.tools(false);
    let root = desk
        .directory
        .path()
        .canonicalize()
        .expect("the root resolves");
    let absolute = format!("{}/notes/a.txt", root.display());

    let output = call(
        &tools,
        "file_write",
        json!({"path": absolute, "content": "a\n"}),
    )
    .await;

    assert_eq!(
        output.text,
        "Wrote 2 bytes to notes/a.txt\nLines changed:\n\"notes/a.txt\" +1 -0"
    );
    assert_eq!(desk.text("notes/a.txt").as_deref(), Some("a\n"));
}

/// A link that leads out of the root is the environment's to refuse; its
/// refusal reads back under the gate's code.
#[cfg(unix)]
#[tokio::test]
async fn a_link_out_of_the_root_is_refused_under_the_gates_code() {
    let desk = Desk::new();
    let outside = tempfile::tempdir().expect("another scratch directory");
    std::fs::write(outside.path().join("secret.txt"), "s\n").expect("written");
    std::os::unix::fs::symlink(outside.path(), desk.path("out")).expect("linked");
    let tools = desk.tools(false);

    let read = call(&tools, "file_read", json!({"path": "out/secret.txt"})).await;
    let write = call(
        &tools,
        "file_write",
        json!({"path": "out/new.txt", "content": "x"}),
    )
    .await;

    assert_eq!(
        read.text,
        "[path_not_allowed] the path leaves the working directory"
    );
    assert_eq!(code(&write), Some("path_not_allowed"));
    assert!(
        !outside.path().join("new.txt").exists(),
        "nothing was written outside"
    );
}
