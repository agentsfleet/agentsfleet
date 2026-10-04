#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use afr_tools::catalog::{FILE_READ, UPDATE_PLAN, WEB_SEARCH};
use afr_tools::stub::{STUB_PATH, Stub};
use afr_tools::{
    Catalog, Entry, Lease, Runtime, Schema, Tool, ToolContext, ToolErrorCode, ToolOutput,
};
use bytes::Bytes;

use super::Router;

/// An executor that counts the calls that crossed into it.
#[derive(Debug, Default)]
struct Counting {
    calls: AtomicUsize,
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
    async fn list_dir(&self, _path: &str) -> afr_executor::Result<Listing> {
        self.crossed();
        Ok(Listing::default())
    }
}

fn catalog() -> Catalog {
    Catalog::new(vec![Stub::boxed(&UPDATE_PLAN), Stub::boxed(&FILE_READ)])
}

#[tokio::test]
async fn test_router_sends_each_tool_to_its_runtime() {
    let catalog = catalog();
    let selection = catalog.select(&["update_plan", "file_read"]).unwrap();
    let executor = Counting::default();
    let router = Router::new(&selection, Some(&executor));

    let supervised = router
        .dispatch("update_plan", &serde_json::json!({}), &mut Lease::default())
        .await;
    assert_eq!(supervised.error_code, None);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "the supervisor ran it"
    );

    let sandboxed = router
        .dispatch("file_read", &serde_json::json!({}), &mut Lease::default())
        .await;
    assert_eq!(sandboxed.error_code, None);
    assert_eq!(sandboxed.text, "file_read");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1, "one call crossed");
}

#[tokio::test]
async fn a_name_the_lease_was_not_offered_is_a_tool_error() {
    let catalog = catalog();
    let selection = catalog.select(&["update_plan"]).unwrap();
    let executor = Counting::default();
    let router = Router::new(&selection, Some(&executor));

    for name in ["shell", "file_read", "teleport"] {
        let output = router
            .dispatch(name, &serde_json::json!({}), &mut Lease::default())
            .await;

        assert_eq!(output.error_code, Some(ToolErrorCode::NotOffered));
        assert!(
            output.text.starts_with("[tool_not_offered] "),
            "{}",
            output.text
        );
        assert!(output.text.contains(name));
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_provider_hosted_tool_reaching_the_router_names_its_code() {
    let catalog = catalog();
    let selection = catalog.select(&[WEB_SEARCH.name()]).unwrap();
    let router = Router::new(&selection, None);

    let output = router
        .dispatch("web_search", &serde_json::json!({}), &mut Lease::default())
        .await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::HostedToolUnavailable)
    );
    assert!(
        output
            .text
            .starts_with("[hosted_tool_unavailable] web_search ")
    );
}

#[tokio::test]
async fn a_sandbox_side_call_without_a_sandbox_is_a_tool_error() {
    let catalog = catalog();
    let selection = catalog.select(&["file_read"]).unwrap();
    let router = Router::new(&selection, None);

    let output = router
        .dispatch("file_read", &serde_json::json!({}), &mut Lease::default())
        .await;

    assert_eq!(output.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert!(output.text.contains("file_read"));
}

/// A handler claiming the provider's runtime for a tool the catalog publishes
/// as a supervisor one, counting every call that reaches it.
#[derive(Debug)]
struct ClaimsProvider {
    served: Box<dyn Tool>,
    calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Tool for ClaimsProvider {
    fn entry(&self) -> &'static Entry {
        self.served.entry()
    }

    fn schema(&self) -> &Schema {
        self.served.schema()
    }

    async fn call(
        &self,
        arguments: &serde_json::Value,
        context: ToolContext<'_, '_>,
    ) -> ToolOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.served.call(arguments, context).await
    }

    fn runtime(&self) -> Runtime {
        Runtime::Provider
    }
}

#[tokio::test]
async fn a_handler_claiming_the_providers_runtime_is_never_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let claimant = ClaimsProvider {
        served: Stub::boxed(&UPDATE_PLAN),
        calls: Arc::clone(&calls),
    };
    let catalog = Catalog::new(vec![Box::new(claimant)]);
    let selection = catalog.select(&[UPDATE_PLAN.name()]).unwrap();
    let router = Router::new(&selection, None);

    let output = router
        .dispatch(
            UPDATE_PLAN.name(),
            &serde_json::json!({}),
            &mut Lease::default(),
        )
        .await;

    assert_eq!(
        output.error_code,
        Some(ToolErrorCode::HostedToolUnavailable)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the handler never ran");
}
