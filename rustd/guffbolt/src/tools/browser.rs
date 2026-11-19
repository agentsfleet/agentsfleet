//! The browser tools, through the environment's browser.
//!
//! `browser_open`, `browser` and `screenshot` drive the page the environment
//! keeps ([`crate::tools::env::Browser`]). An environment with no browser, or
//! a call with no environment, answers a code naming why, spawns nothing,
//! and the run goes on. A sandboxing embedder whose sandbox cannot start a
//! browser says so in its own words, such as the engine its browser waits
//! for.

use serde_json::Value;

use crate::tool::ToolOutput;
use crate::tools::env::Browser;
use crate::tools::{Context, Handler, code};

/// What a browser call with no environment reads back after its name.
const NO_ENV: &str = "has no environment to run a browser in";
/// The event a refused browser call logs under.
pub const EVENT_REFUSED: &str = "browser_tool_refused";
/// What the three tools tell the model, after what each would do.
macro_rules! until_firecracker {
    () => {
        " Refused on this sandbox engine until the Firecracker engine arrives; the call answers \
         a code and the run continues."
    };
}

/// The call's browser, or its refusal: logged under the run it came from,
/// naming why there is none.
fn browser<'a>(name: &str, context: &Context<'a>) -> Result<&'a dyn Browser, ToolOutput> {
    let why = match context.env.map(|env| env.browser()) {
        Some(Ok(browser)) => return Ok(browser),
        Some(Err(why)) => why,
        None => NO_ENV,
    };
    let lease_id = context.run_id;
    let tool = name;
    let event = EVENT_REFUSED;
    tracing::info!(lease_id, tool, event);
    Err(ToolOutput::failed(
        code::BROWSER_UNAVAILABLE,
        &format!("{tool} {why}"),
    ))
}

/// What a browser call that failed reads back.
fn failed(failure: &crate::tools::env::EnvError) -> ToolOutput {
    ToolOutput::failed(code::BROWSER_UNAVAILABLE, failure.message())
}

/// Opens a page in the browser.
#[derive(Debug, Clone, Copy, Default)]
pub struct BrowserOpen;

impl Handler for BrowserOpen {
    const NAME: &'static str = "browser_open";
    const DESCRIPTION: &'static str = concat!(
        "Open a page in a browser inside the sandbox.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, &context) {
            Ok(browser) => browser
                .open(&arguments)
                .await
                .map_or_else(|failure| failed(&failure), ToolOutput::succeeded),
            Err(refused) => refused,
        }
    }
}

/// Drives the open page.
#[derive(Debug, Clone, Copy, Default)]
pub struct Browse;

impl Handler for Browse {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str = concat!(
        "Act on the open page in the sandbox's browser: click, type, scroll, read.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, &context) {
            Ok(browser) => browser
                .act(&arguments)
                .await
                .map_or_else(|failure| failed(&failure), ToolOutput::succeeded),
            Err(refused) => refused,
        }
    }
}

/// Captures the open page.
#[derive(Debug, Clone, Copy, Default)]
pub struct Screenshot;

impl Handler for Screenshot {
    const NAME: &'static str = "screenshot";
    const DESCRIPTION: &'static str = concat!(
        "Take a screenshot of the open page in the sandbox's browser.",
        until_firecracker!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, &context) {
            Ok(browser) => match browser.screenshot(&arguments).await {
                Ok(image) => {
                    let bytes = image.data.len();
                    let kind = image.kind.media_type();
                    ToolOutput::succeeded(format!(
                        "Attached a screenshot ({bytes} bytes, {kind}) for your next turn"
                    ))
                    .with_image(image)
                }
                Err(failure) => failed(&failure),
            },
            Err(refused) => refused,
        }
    }
}
