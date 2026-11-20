//! The file tools over a real directory: `LocalEnv` rooted in a scratch
//! directory, the tools built with `Tools::builder().with_env`, and each call
//! dispatched the way a loop dispatches it. Every answer is read as the model
//! reads it, and every write is checked on disk.

use std::sync::Arc;

use guffbolt::tools::env::{
    Env, EnvError, EnvErrorKind, EnvFuture, FileContent, Listing, Process, ProcessId, Spawn,
};
use guffbolt::tools::search::{Deferred, ToolSearch};
use guffbolt::tools::{Context, ContextTool, LocalEnv, ToolsConfig, Typed};
use guffbolt::{CallRef, ImageKind, ToolOutput, ToolSpec, Tools};
use serde_json::{Value, json};
use tempfile::TempDir;

mod apply_patch;
mod apply_patch_refusals;
mod grep;
mod hashed;
mod image;
mod listing;
mod path_gate;
mod read;
mod read_only_disk;
mod tool_search;
mod write;

/// What one read of the environment carries: 8 MiB.
const ONE_READ: usize = 8 * 1024 * 1024;
/// The most an image may weigh: 5 MiB.
const IMAGE_MAX: usize = 5 * 1024 * 1024;
/// What `ls` and `find` say when they stop at their limit.
const LIMIT_REACHED: &str = "... limit reached; narrow the path or the pattern";
/// What a refused path reads back after it.
const LEAVES: &str = "leaves the working directory";

/// A scratch working directory and the environment rooted in it.
struct Desk {
    directory: TempDir,
    env: LocalEnv,
}

impl Desk {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let env = LocalEnv::new(directory.path()).expect("the directory exists");
        Self { directory, env }
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.directory.path().join(relative)
    }

    /// Writes `bytes` at `relative`, making its directories.
    fn put(&self, relative: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("directories made");
        }
        std::fs::write(path, bytes).expect("written");
    }

    /// The text at `relative`, or `None` when nothing is there.
    fn text(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.path(relative)).ok()
    }

    /// Every file tool, `image` showing pictures when `image_input` says so.
    fn tools(&self, image_input: bool) -> Tools<ContextTool<'_>> {
        let config = ToolsConfig::new(["file_*", "apply_patch", "image", "ls", "find", "grep"])
            .expect("names and patterns");
        Tools::builder()
            .with_env(&self.env)
            .with_image_input(image_input)
            .build(&config)
            .expect("the environment is given")
    }
}

/// Calls `name` with `arguments` the way a loop dispatches a call.
async fn call(tools: &Tools<ContextTool<'_>>, name: &str, arguments: Value) -> ToolOutput {
    let call_ref = CallRef {
        step: 1,
        index: 0,
        call_id: "call-1".to_owned(),
    };
    let context = Context {
        call_ref: &call_ref,
        run_id: "run-1",
        wake: None,
    };
    tools.dispatch(name, &arguments, context).await
}

/// The code a failed call carries, by its spelling.
fn code(output: &ToolOutput) -> Option<&'static str> {
    output.error_code.map(guffbolt::ErrorCode::as_str)
}

/// The tag a `file_read_hashed` answer gives line `number`, as `L<n>:<hhh>`.
fn tag_of(read: &str, number: usize) -> String {
    let prefix = format!("L{number}:");
    read.lines()
        .find(|line| line.starts_with(&prefix))
        .and_then(|line| line.split_once('|'))
        .map(|(tag, _text)| tag.to_owned())
        .expect("the read tags the line")
}

/// A small tree: files at two depths, a dotfile, `.git`, and a link.
fn tree(desk: &Desk) {
    desk.put("README.md", "read me\n");
    desk.put(".env", "KEY=1\n");
    desk.put("src/a.rs", "fn alpha() {}\n// TODO: tidy\n");
    desk.put("src/b.rs", "fn beta() {}\n");
    desk.put("src/deep/c.rs", "fn gamma() { todo!() }\n");
    desk.put("docs/guide.txt", "Caf\u{00e9} guide\nTODO later\n");
    desk.put(".git/HEAD", "TODO: never read\n");
    #[cfg(unix)]
    std::os::unix::fs::symlink(desk.path("src/a.rs"), desk.path("link.rs")).expect("linked");
}

/// The patch around a body of hunks.
fn wrapped(body: &str) -> String {
    format!("*** Begin Patch\n{body}\n*** End Patch")
}
