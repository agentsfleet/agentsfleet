//! The browser tools, through the environment's browser.
//!
//! `browser_open`, `browser` and `screenshot` drive the page the environment
//! keeps ([`crate::tools::env::Browser`]). An environment with no browser
//! answers a code naming why, spawns nothing, and the run goes on. An
//! environment that cannot start a browser says so in its own words.

use serde_json::Value;

use crate::tool::ToolOutput;
use crate::tools::env::{Browser, Env};
use crate::tools::{Context, Handler, code};
/// The event a refused browser call logs under.
pub const EVENT_REFUSED: &str = "browser_tool_refused";
/// What the three tools tell the model, after what each would do.
macro_rules! when_unavailable {
    () => {
        " An environment with no browser refuses the call with a code, and the run continues."
    };
}

/// `env`'s browser, or its refusal: logged under the run the call came
/// from, naming why there is none.
fn browser<'a>(
    name: &str,
    env: &'a dyn Env,
    context: &Context<'_>,
) -> Result<&'a dyn Browser, ToolOutput> {
    let why = match env.browser() {
        Ok(browser) => return Ok(browser),
        Err(why) => why,
    };
    let run_id = context.run_id;
    let tool = name;
    let event = EVENT_REFUSED;
    tracing::info!(run_id, tool, event);
    Err(ToolOutput::failed(
        code::BROWSER_UNAVAILABLE,
        &format!("{tool} {why}"),
    ))
}

/// What a browser call that failed reads back.
fn failed(failure: &crate::tools::env::EnvError) -> ToolOutput {
    ToolOutput::failed(code::BROWSER_UNAVAILABLE, failure.message())
}

backed! {
    /// Opens a page in the browser.
    BrowserOpen over Env
}

impl Handler for BrowserOpen<'_> {
    const NAME: &'static str = "browser_open";
    const DESCRIPTION: &'static str = concat!(
        "Open a page in the environment's browser.",
        when_unavailable!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, self.backend, &context) {
            Ok(browser) => browser
                .open(&arguments)
                .await
                .map_or_else(|failure| failed(&failure), ToolOutput::succeeded),
            Err(refused) => refused,
        }
    }
}

backed! {
    /// Drives the open page.
    Browse over Env
}

impl Handler for Browse<'_> {
    const NAME: &'static str = "browser";
    const DESCRIPTION: &'static str = concat!(
        "Act on the open page in the environment's browser: click, type, scroll, read.",
        when_unavailable!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, self.backend, &context) {
            Ok(browser) => browser
                .act(&arguments)
                .await
                .map_or_else(|failure| failed(&failure), ToolOutput::succeeded),
            Err(refused) => refused,
        }
    }
}

backed! {
    /// Captures the open page.
    Screenshot over Env
}

impl Handler for Screenshot<'_> {
    const NAME: &'static str = "screenshot";
    const DESCRIPTION: &'static str = concat!(
        "Take a screenshot of the open page in the environment's browser.",
        when_unavailable!()
    );
    type Arguments = Value;

    async fn run(&self, arguments: Value, context: Context<'_>) -> ToolOutput {
        match browser(Self::NAME, self.backend, &context) {
            Ok(browser) => match browser.screenshot(&arguments).await {
                Ok(image) => {
                    let bytes = image.data.len();
                    let kind = image.kind.media_type();
                    ToolOutput::succeeded(format!(
                        "Attached a screenshot ({bytes} bytes, {kind}) for your next step"
                    ))
                    .with_image(image)
                }
                Err(failure) => failed(&failure),
            },
            Err(refused) => refused,
        }
    }
}
