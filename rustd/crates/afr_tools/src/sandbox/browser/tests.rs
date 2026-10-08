#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use serde_json::json;

use super::EVENT_REFUSED;
use crate::catalog::{BROWSER, BROWSER_OPEN, FILE_READ, SCREENSHOT};
use crate::lease::Lease;
use crate::runtime::ToolErrorCode;
use crate::sandbox::ScriptedExecutor;
use crate::testing::{Live, call, call_in, hosted, offered};

/// The argument name a file call spells.
const PATH: &str = "path";
/// A file the lease's other tools still reach.
const PAGE: &str = "page.txt";
const STILL_HERE: &str = "still here";

/// The tools the refusal log names, in order; every entry names its lease.
fn refused_tools_logged(capture: &Capture) -> Vec<String> {
    let refusals: Vec<_> = capture
        .events()
        .into_iter()
        .filter(|event| event.field("event") == Some(EVENT_REFUSED))
        .collect();
    assert!(
        refusals
            .iter()
            .all(|event| event.field("lease_id").is_some()),
        "{refusals:?}"
    );
    refusals
        .iter()
        .filter_map(|event| event.field("tool").map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn test_browser_tools_refuse_until_firecracker() {
    let capture = Capture::install();
    let scripted = ScriptedExecutor::default();
    let live = Live::start().await;
    std::fs::write(live.root.join(PAGE), STILL_HERE).unwrap();
    let (catalog, _sent) = hosted();
    let selection = catalog
        .select(&[
            BROWSER_OPEN.name(),
            BROWSER.name(),
            SCREENSHOT.name(),
            FILE_READ.name(),
        ])
        .unwrap();
    let lease = Lease::default();

    let mut refusals = Vec::with_capacity(3);
    for (entry, arguments) in [
        (&BROWSER_OPEN, json!({"url": "https://example.com"})),
        (&BROWSER, json!({"action": "click", "selector": "#go"})),
        (&SCREENSHOT, json!({})),
    ] {
        let tool = offered(&selection, entry);
        refusals.push((
            entry.name(),
            call_in(tool, &scripted, &lease, arguments).await,
        ));
    }
    let read = call_in(
        offered(&selection, &FILE_READ),
        &live.client,
        &lease,
        json!({PATH: PAGE}),
    )
    .await;

    for (name, refused) in &refusals {
        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::BrowserUnavailable),
            "{name}: {refused:?}"
        );
        assert_eq!(
            refused.text,
            format!(
                "[browser_unavailable] {name} waits for the Firecracker engine: Chromium cannot \
                 start inside this sandbox"
            )
        );
    }
    assert!(scripted.spawned().is_empty(), "no process was started");
    assert_eq!(read.text, STILL_HERE, "the lease's other tools still run");
    assert_eq!(
        refused_tools_logged(&capture),
        ["browser_open", "browser", "screenshot"]
    );
    live.stop().await;
}

/// Any argument shape, and a run with no sandbox at all, are refused alike:
/// the call never reaches a parser or an executor that would mind.
#[tokio::test]
async fn any_arguments_and_no_sandbox_are_refused_the_same_way() {
    let (catalog, _sent) = hosted();
    let selection = catalog.select(&[BROWSER.name()]).unwrap();
    let lease = Lease::default();

    for arguments in [
        json!(null),
        json!("click"),
        json!([1, 2]),
        json!({"nested": {"deep": true}}),
    ] {
        let refused = call(offered(&selection, &BROWSER), &lease, arguments).await;

        assert_eq!(
            refused.error_code,
            Some(ToolErrorCode::BrowserUnavailable),
            "{refused:?}"
        );
    }
}
