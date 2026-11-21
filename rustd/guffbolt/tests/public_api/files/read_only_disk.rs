//! The file tools over a disk that refuses every write.

use super::*;

/// [`LocalEnv`] over the desk, whose every write, append and removal the
/// disk refuses, as a full or read-only disk does once a file was read.
#[derive(Debug)]
struct ReadOnly(LocalEnv);

/// What the refusing disk says.
const READ_ONLY: &str = "the disk is read-only";

/// The refusing disk's answer.
fn refused<'a, T: Send + 'a>() -> EnvFuture<'a, T> {
    Box::pin(std::future::ready(Err(EnvError::new(
        EnvErrorKind::Unavailable,
        READ_ONLY,
    ))))
}

impl Env for ReadOnly {
    fn root(&self) -> &str {
        self.0.root()
    }

    fn spawn<'a>(&'a self, spawn: &'a Spawn) -> EnvFuture<'a, Process> {
        self.0.spawn(spawn)
    }

    fn write(&self, process: ProcessId, data: bytes::Bytes) -> EnvFuture<'_, ()> {
        self.0.write(process, data)
    }

    fn kill(&self, process: ProcessId) -> EnvFuture<'_, ()> {
        self.0.kill(process)
    }

    fn read_file<'a>(&'a self, path: &'a str, max_bytes: u64) -> EnvFuture<'a, FileContent> {
        self.0.read_file(path, max_bytes)
    }

    fn write_file<'a>(&'a self, _path: &'a str, _data: bytes::Bytes) -> EnvFuture<'a, ()> {
        refused()
    }

    fn append_file<'a>(&'a self, _path: &'a str, _data: bytes::Bytes) -> EnvFuture<'a, ()> {
        refused()
    }

    fn delete_file<'a>(&'a self, _path: &'a str) -> EnvFuture<'a, ()> {
        refused()
    }

    fn list_dir<'a>(&'a self, path: &'a str) -> EnvFuture<'a, Listing> {
        self.0.list_dir(path)
    }
}

/// An edit, a tagged edit and a patch's removal the disk refuses each read
/// the disk's refusal, and the file stays as it was.
#[tokio::test]
async fn an_edit_or_a_removal_the_disk_refuses_reads_its_refusal() {
    let desk = Desk::new();
    desk.put("f.txt", "a\nb\n");
    let env = ReadOnly(LocalEnv::new(desk.path("")).expect("the directory exists"));
    let config = ToolsConfig::new([
        "file_edit",
        "file_read_hashed",
        "file_edit_hashed",
        "apply_patch",
    ])
    .expect("four names");
    let tools = Tools::builder()
        .with_env(&env)
        .build(&config)
        .expect("the environment is given");

    let edited = call(
        &tools,
        "file_edit",
        json!({"path": "f.txt", "old_text": "a", "new_text": "A"}),
    )
    .await;
    let read = call(&tools, "file_read_hashed", json!({"path": "f.txt"})).await;
    let hashed = call(
        &tools,
        "file_edit_hashed",
        json!({"path": "f.txt", "target": tag_of(&read.text, 1), "new_text": "A"}),
    )
    .await;
    let patched = call(
        &tools,
        "apply_patch",
        json!({"patch": "*** Begin Patch\n*** Delete File: f.txt\n*** End Patch"}),
    )
    .await;

    let refusal = format!("[sandbox_unavailable] {READ_ONLY}");
    assert_eq!(edited.text, refusal);
    assert_eq!(hashed.text, refusal);
    assert_eq!(patched.text, refusal);
    assert_eq!(desk.text("f.txt").as_deref(), Some("a\nb\n"));
}

/// Two hunks on the one path count their lines together under it.
#[tokio::test]
async fn apply_patch_counts_two_hunks_on_one_path_together() {
    let desk = Desk::new();
    desk.put("f.txt", "a\nb\n");
    let tools = desk.tools(false);
    let patch = "*** Begin Patch\n\
                 *** Update File: f.txt\n\
                 -a\n\
                 +A\n\
                 *** Update File: f.txt\n\
                 @@ A\n\
                 -b\n\
                 +B\n\
                 *** End Patch";

    let output = call(&tools, "apply_patch", json!({"patch": patch})).await;

    assert_eq!(
        output.text,
        "Success. Updated the following files:\nM f.txt\nM f.txt\nLines changed:\n\"f.txt\" +2 -2"
    );
    assert_eq!(desk.text("f.txt").as_deref(), Some("A\nB\n"));
}

/// A glob too large to compile is refused as an argument, and nothing is
/// searched.
#[tokio::test]
async fn a_glob_too_large_to_compile_is_refused() {
    let desk = Desk::new();
    tree(&desk);
    let tools = desk.tools(false);
    let huge = "?".repeat(100_000);

    let found = call(&tools, "find", json!({"pattern": huge})).await;

    assert_eq!(code(&found), Some("invalid_arguments"), "{}", found.text);
}
