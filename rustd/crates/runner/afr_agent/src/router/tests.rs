#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::tool_trace::ToolCallStatus;
use afr_executor::{
    Executor, ExecutorSlot, FileContent, Listing, Process, ProcessId, Spawn, Unavailable,
};
use afr_tools::catalog::{FILE_READ, UPDATE_PLAN, WEB_SEARCH};
use afr_tools::stub::{STUB_PATH, Stub};
use afr_tools::{
    Catalog, CatalogTool, Entry, Lease, Runtime, Selection, ToolContext, ToolErrorCode,
};
use bytes::Bytes;
use guffbolt::Message;
use guffbolt::{Describe, Tool, ToolFuture, ToolSpec};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::Routed;
use crate::fixture::{Script, call, lease, say, unbounded};
use crate::harness::tests::{completions, drive, engine};

/// An executor that counts the calls that crossed into it.
#[derive(Debug, Default)]
pub(super) struct Counting {
    pub(super) calls: AtomicUsize,
}

impl Counting {
    fn crossed(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl Executor for Counting {
    async fn spawn(&self, _spawn: &Spawn) -> afr_executor::Result<Process> {
        self.crossed();
        Err(std::io::Error::other("no processes here").into())
    }
    async fn write(&self, _process: ProcessId, _data: Bytes) -> afr_executor::Result<()> {
        self.crossed();
        Ok(())
    }
    async fn kill(&self, _process: ProcessId) -> afr_executor::Result<()> {
        self.crossed();
        Ok(())
    }
    async fn read_file(&self, path: &str, _max: u64) -> afr_executor::Result<FileContent> {
        assert_eq!(path, STUB_PATH);
        self.crossed();
        Ok(FileContent {
            data: Bytes::new(),
            truncated: false,
        })
    }
    async fn write_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        self.crossed();
        Ok(())
    }
    async fn append_file(&self, _path: &str, _data: Bytes) -> afr_executor::Result<()> {
        self.crossed();
        Ok(())
    }
    async fn delete_file(&self, _path: &str) -> afr_executor::Result<()> {
        self.crossed();
        Ok(())
    }
    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        self.crossed();
        Ok(Listing::default())
    }
}

pub(super) fn catalog() -> Catalog {
    Catalog::new(vec![Stub::boxed(&UPDATE_PLAN), Stub::boxed(&FILE_READ)])
}

/// The handler `selection` offers for `entry`, as a loop routes it.
pub(super) fn routed<'c>(selection: &Selection<'c>, entry: &Entry) -> Routed<'c> {
    Routed::new(selection.tool(entry.name()).unwrap(), entry.runtime())
}

#[tokio::test]
async fn test_router_sends_each_tool_to_its_runtime() {
    let catalog = catalog();
    let selection = catalog.select(&["update_plan", "file_read"]).unwrap();
    let executor = Counting::default();
    let slot = ExecutorSlot::ready(&executor);
    let (arguments, lease) = (json!({}), Lease::default());

    let supervised = routed(&selection, &UPDATE_PLAN)
        .run(&arguments, &slot, &lease)
        .await;
    assert_eq!(supervised.error_code, None);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "the supervisor ran it"
    );

    let sandboxed = routed(&selection, &FILE_READ)
        .run(&arguments, &slot, &lease)
        .await;
    assert_eq!(sandboxed.error_code, None);
    assert_eq!(sandboxed.text, "file_read");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1, "one call crossed");
}

#[tokio::test]
async fn a_sandbox_side_call_without_a_sandbox_is_a_tool_error() {
    let catalog = catalog();
    let selection = catalog.select(&["file_read"]).unwrap();
    let slot = ExecutorSlot::unavailable(Unavailable::NoSandbox);

    let output = routed(&selection, &FILE_READ)
        .run(&json!({}), &slot, &Lease::default())
        .await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::SandboxUnavailable.into())
    );
    assert!(output.text.contains("file_read"));
}

/// The model calls a tool its lease was not offered, and one the provider
/// hosts but did not run: each reads its own refusal and the run goes on.
#[tokio::test]
async fn a_name_not_offered_and_a_hosted_name_are_refused_as_tool_errors() {
    let names = ["shell", FILE_READ.name(), "teleport", WEB_SEARCH.name()];
    let script = Script::new([
        names
            .iter()
            .map(|name| call(name, name, json!({})))
            .collect(),
        vec![say("done")],
    ]);
    let engine = engine(
        vec![Stub::boxed(&UPDATE_PLAN), Stub::boxed(&FILE_READ)],
        &script,
    );
    let lease = lease(&[UPDATE_PLAN.name(), WEB_SEARCH.name()], unbounded());

    let (output, frames) = drive(&engine, &lease, &CancellationToken::new()).await;

    assert_eq!(output.result.content, "done");
    let results: Vec<String> = script.sent()[1]
        .messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.clone()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect();
    assert_eq!(results.len(), names.len(), "every call answered");
    for (name, result) in names.iter().take(3).zip(&results) {
        assert!(
            result.starts_with(&format!("[tool_not_offered] {name} ")),
            "{result}"
        );
    }
    assert!(
        results[3].starts_with("[hosted_tool_unavailable] web_search "),
        "{}",
        results[3]
    );
    assert!(
        completions(&frames)
            .iter()
            .all(|(_id, status)| *status == ToolCallStatus::Failed)
    );
}

/// A handler that counts every call reaching it, then serves `served`'s.
#[derive(Debug)]
struct Counted {
    served: Box<CatalogTool>,
    calls: Arc<AtomicUsize>,
}

impl Describe for Counted {
    fn spec(&self) -> ToolSpec<'_> {
        self.served.spec()
    }
}

impl<'call, 'run> Tool<ToolContext<'call, 'run>> for Counted {
    fn call<'a>(
        &'a self,
        arguments: &'a serde_json::Value,
        cx: ToolContext<'call, 'run>,
    ) -> ToolFuture<'a>
    where
        ToolContext<'call, 'run>: 'a,
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.served.call(arguments, cx)
    }
}

/// A runtime is its published entry's, so no handler claims the provider's;
/// a handler routed as the provider's anyway is never run.
#[tokio::test]
async fn a_handler_routed_as_the_providers_is_never_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Counted {
        served: Stub::boxed(&UPDATE_PLAN),
        calls: Arc::clone(&calls),
    };
    let slot = ExecutorSlot::unavailable(Unavailable::NoSandbox);

    let output = Routed::new(&counted, Runtime::Provider)
        .run(&json!({}), &slot, &Lease::default())
        .await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::HostedToolUnavailable.into())
    );
    assert!(
        output
            .text
            .starts_with("[hosted_tool_unavailable] update_plan ")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the handler never ran");
}
