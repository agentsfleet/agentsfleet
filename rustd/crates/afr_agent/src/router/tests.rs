#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};

use afr_executor::{Executor, FileContent, Listing, Process, ProcessId, Spawn};
use afr_tools::catalog::{CALCULATOR, FILE_READ, WEB_SEARCH};
use afr_tools::stub::{STUB_PATH, Stub};
use afr_tools::{Catalog, ToolErrorCode};
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
    Catalog::new(vec![Stub::boxed(&CALCULATOR), Stub::boxed(&FILE_READ)])
}

fn arguments() -> serde_json::Value {
    serde_json::json!({})
}

#[tokio::test]
async fn test_router_sends_each_tool_to_its_runtime() {
    let catalog = catalog();
    let selection = catalog.select(&["calculator", "file_read"]).unwrap();
    let executor = Counting::default();
    let router = Router::new(&selection, Some(&executor));

    let supervised = router.dispatch("calculator", &arguments()).await;
    assert_eq!(supervised.error_code, None);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "the supervisor ran it"
    );

    let sandboxed = router.dispatch("file_read", &arguments()).await;
    assert_eq!(sandboxed.error_code, None);
    assert_eq!(sandboxed.text, "file_read");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1, "one call crossed");
}

#[tokio::test]
async fn a_name_the_lease_was_not_offered_is_a_tool_error() {
    let catalog = catalog();
    let selection = catalog.select(&["calculator"]).unwrap();
    let executor = Counting::default();
    let router = Router::new(&selection, Some(&executor));

    for name in ["shell", "file_read", "teleport"] {
        let output = router.dispatch(name, &arguments()).await;

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

    let output = router.dispatch("web_search", &arguments()).await;

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

    let output = router.dispatch("file_read", &arguments()).await;

    assert_eq!(output.error_code, Some(ToolErrorCode::SandboxUnavailable));
    assert!(output.text.contains("file_read"));
}
