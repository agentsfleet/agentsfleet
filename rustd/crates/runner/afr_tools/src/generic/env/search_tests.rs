//! `find` and `grep` over the run's sandbox: the real executor, served
//! in-process, behind a stand-in that counts each call it is asked.

#![expect(
    clippy::unwrap_used,
    reason = "test module: a harness that cannot start should fail the test loudly"
)]

use std::sync::{Mutex, PoisonError};

use afr_executor::{
    Executor, FileContent, Found, Listing, Process, ProcessId, Search, Sizes, Spawn,
};
use bytes::Bytes;
use guffbolt::tools::{Context, ContextTool, LocalEnv, ToolsConfig};
use guffbolt::{CallRef, ToolOutput, Tools};
use serde_json::{Value, json};

use crate::SandboxEnv;
use crate::testing::Live;

/// The directories the tree spreads its files across.
const DIRECTORIES: usize = 10;
/// The files in each directory: a thousand in all.
const FILES_EACH: usize = 100;

/// The executor a run's sandbox serves, every call it is asked named in
/// order.
#[derive(Debug)]
struct Counted<'e> {
    executor: &'e dyn Executor,
    calls: Mutex<Vec<&'static str>>,
}

impl<'e> Counted<'e> {
    fn new(executor: &'e dyn Executor) -> Self {
        Self {
            executor,
            calls: Mutex::default(),
        }
    }

    fn asked(&self, method: &'static str) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(method);
    }

    /// The calls asked since the last time this was read.
    fn taken(&self) -> Vec<&'static str> {
        std::mem::take(&mut *self.calls.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[async_trait::async_trait]
impl Executor for Counted<'_> {
    fn sizes(&self) -> Sizes {
        self.executor.sizes()
    }

    async fn spawn(&self, spawn: &Spawn) -> afr_executor::Result<Process> {
        self.asked("process/spawn");
        self.executor.spawn(spawn).await
    }

    async fn write(&self, process: ProcessId, data: Bytes) -> afr_executor::Result<()> {
        self.asked("process/write");
        self.executor.write(process, data).await
    }

    async fn kill(&self, process: ProcessId) -> afr_executor::Result<()> {
        self.asked("process/kill");
        self.executor.kill(process).await
    }

    async fn read_file(&self, path: &str, max_bytes: u64) -> afr_executor::Result<FileContent> {
        self.asked("fs/read");
        self.executor.read_file(path, max_bytes).await
    }

    async fn list_dir(&self, path: &str) -> afr_executor::Result<Listing> {
        self.asked("fs/list");
        self.executor.list_dir(path).await
    }

    async fn search(&self, search: &Search) -> afr_executor::Result<Found> {
        self.asked("fs/search");
        self.executor.search(search).await
    }
}

/// Calls `name` with `arguments` the way a loop dispatches a call.
async fn call(tools: &Tools<ContextTool<'_>>, name: &str, arguments: &Value) -> ToolOutput {
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
    tools.dispatch(name, arguments, context).await
}

/// `find` and `grep`, built over `env`.
fn tools(env: &dyn guffbolt::tools::Env) -> Tools<ContextTool<'_>> {
    Tools::builder()
        .with_env(env)
        .build(&ToolsConfig::new(["find", "grep"]).unwrap())
        .unwrap()
}

/// A thousand files under `live`'s workspace, every seventh holding the
/// needle on its second line; what `grep` for it answers, in path order, and
/// what `find` answers.
fn planted(live: &Live) -> (String, String) {
    let mut lines = Vec::new();
    let mut paths = Vec::new();
    for directory in 0..DIRECTORIES {
        let place = live.root.join(format!("d{directory}"));
        std::fs::create_dir_all(&place).unwrap();
        for file in 0..FILES_EACH {
            let number = directory * FILES_EACH + file;
            let path = format!("d{directory}/f{file}.txt");
            let needle = if number.is_multiple_of(7) {
                "needle"
            } else {
                "hay"
            };
            std::fs::write(
                live.root.join(&path),
                format!("line {number}\n{needle} {number}\n"),
            )
            .unwrap();
            if number.is_multiple_of(7) {
                lines.push(format!("{path}:2: needle {number}"));
            }
            paths.push(path);
        }
    }
    lines.sort_unstable();
    paths.sort_unstable();
    (lines.join("\n"), paths.join("\n"))
}

#[tokio::test]
async fn a_search_over_a_tree_is_one_executor_call() {
    let live = Live::start().await;
    let (lines, paths) = planted(&live);
    let counted = Counted::new(&live.client);
    let sandbox = SandboxEnv::new(&counted);
    let local = LocalEnv::new(&live.root).unwrap();
    let grep = json!({"pattern": "needle \\d+", "limit": 1000});
    let find = json!({"pattern": "*.txt", "limit": 1000});

    let grepped = call(&tools(&sandbox), "grep", &grep).await;
    let grep_calls = counted.taken();
    let found = call(&tools(&sandbox), "find", &find).await;
    let find_calls = counted.taken();
    let on_the_host = call(&tools(&local), "grep", &grep).await;

    assert_eq!(grep_calls, ["fs/search"]);
    assert_eq!(find_calls, ["fs/search"]);
    assert_eq!(
        grepped.text, lines,
        "the lines the walk found, in path order"
    );
    assert_eq!(found.text, paths);
    assert_eq!(on_the_host.text, grepped.text, "the host searches alike");
    live.stop().await;
}

#[tokio::test]
async fn a_search_skips_gitignored_paths() {
    let live = Live::start().await;
    std::fs::write(live.root.join(".gitignore"), "target/\n").unwrap();
    for place in ["src", "target/debug"] {
        std::fs::create_dir_all(live.root.join(place)).unwrap();
        std::fs::write(live.root.join(place).join("main.rs"), "fn main() {}\n").unwrap();
    }
    let sandbox = SandboxEnv::new(&live.client);

    let grepped = call(&tools(&sandbox), "grep", &json!({"pattern": "fn main"})).await;
    let found = call(&tools(&sandbox), "find", &json!({"pattern": "**/main.rs"})).await;

    assert_eq!(grepped.text, "src/main.rs:1: fn main() {}");
    assert_eq!(found.text, "src/main.rs");
    live.stop().await;
}

#[tokio::test]
async fn grep_context_surrounds_the_last_match_in_one_executor_call() {
    let live = Live::start().await;
    std::fs::write(
        live.root.join("notes"),
        format!(
            "before\n{}hit\nhit again\nafter\n",
            "x".repeat(live.client.sizes().max_frame_bytes)
        ),
    )
    .unwrap();
    let counted = Counted::new(&live.client);
    let sandbox = SandboxEnv::new(&counted);
    let answer = call(
        &tools(&sandbox),
        "grep",
        &json!({"pattern":"hit", "context":1, "limit":1}),
    )
    .await;
    assert_eq!(
        answer.text,
        format!(
            "notes:1: before\nnotes:2: {} [line cut at 500 characters]\nnotes:3: hit again\n... limit reached; narrow the path or the pattern",
            "x".repeat(500)
        )
    );
    assert_eq!(counted.taken(), ["fs/search"]);
    live.stop().await;
}
