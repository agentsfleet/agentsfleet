//! The browser tools, refused until the Firecracker engine.
//!
//! Spike S1 (`docs/v2/reviews/m211-toolbox-spikes.md`) showed Chromium cannot
//! start inside this sandbox: its own sandbox needs a user namespace or a
//! setuid helper, bubblewrap refuses both here, and `--no-sandbox` is never
//! passed. So `browser_open`, `browser` and `screenshot` answer a code naming
//! the engine they wait for, spawn nothing, and the run goes on; the handlers
//! that drive Chromium over its pipes arrive with that engine, where each
//! lease has its own kernel.

use serde_json::Value;

use crate::catalog::{BROWSER, BROWSER_OPEN, Entry, SCREENSHOT};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// What each browser tool reads back after its name.
const WAITS_FOR_FIRECRACKER: &str =
    "waits for the Firecracker engine: Chromium cannot start inside this sandbox";
/// The event a refused browser call logs under.
const EVENT_REFUSED: &str = "browser_tool_refused";
/// What the three tools tell the model, after what each would do.
macro_rules! until_firecracker {
    () => {
        " Refused on this sandbox engine until the Firecracker engine arrives; the call answers \
         a code and the run continues."
    };
}
/// Refuses `entry`'s call, naming the engine it waits for, and logs the
/// refusal under the lease it came from.
fn refused(entry: &Entry, context: &ToolContext<'_, '_>) -> ToolOutput {
    let lease_id = context.lease.egress.lease_id();
    let tool = entry.name();
    let event = EVENT_REFUSED;
    tracing::info!(lease_id, tool, event);
    ToolOutput::failed(
        ToolErrorCode::BrowserUnavailable,
        &format!("{tool} {WAITS_FOR_FIRECRACKER}"),
    )
}

/// Opens a page in Chromium.
#[derive(Debug)]
pub(crate) struct BrowserOpen;

#[async_trait::async_trait]
impl Handler for BrowserOpen {
    const ENTRY: &'static Entry = &BROWSER_OPEN;
    const DESCRIPTION: &'static str = concat!(
        "Open a page in a browser inside the sandbox.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, _arguments: Value, context: ToolContext<'_, '_>) -> ToolOutput {
        refused(Self::ENTRY, &context)
    }
}

/// Drives the open page.
#[derive(Debug)]
pub(crate) struct Browser;

#[async_trait::async_trait]
impl Handler for Browser {
    const ENTRY: &'static Entry = &BROWSER;
    const DESCRIPTION: &'static str = concat!(
        "Act on the open page in the sandbox's browser: click, type, scroll, read.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, _arguments: Value, context: ToolContext<'_, '_>) -> ToolOutput {
        refused(Self::ENTRY, &context)
    }
}

/// Captures the open page.
#[derive(Debug)]
pub(crate) struct Screenshot;

#[async_trait::async_trait]
impl Handler for Screenshot {
    const ENTRY: &'static Entry = &SCREENSHOT;
    const DESCRIPTION: &'static str = concat!(
        "Take a screenshot of the open page in the sandbox's browser.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, _arguments: Value, context: ToolContext<'_, '_>) -> ToolOutput {
        refused(Self::ENTRY, &context)
    }
}

#[cfg(test)]
#[path = "browser/tests.rs"]
mod tests;
