//! The rig model one lease's route speaks.
//!
//! Messages and Responses are rig's own clients. Chat takes rig's dialect for
//! a vendor rig knows (its path, its quirks, how it carries reasoning back) and
//! a plain OpenAI-compatible gateway at the route's base for everything else.
//! The base is always the registry's, so a host is chosen by the runner's
//! table and never by the library. Messages caches the system prompt, the
//! tools and the conversation's tail, so a loop re-sending its growing
//! conversation pays full price for the new turn only.

use rig_core::DynModel;
use rig_core::operation::Completion;
use rig_core::providers::anthropic::AnthropicConfig;
use rig_core::providers::openai::OpenAIConfig;
use rig_core::providers::openai::wire::{Dialect, by_name};

use crate::registry::{Route, Wire};
use crate::transport::Transport;

/// The completion tokens a Messages turn may spend, the bound the Zig runner
/// sent; Messages requires one.
const MAX_TOKENS: u64 = 8192;
/// The dialect of a chat provider rig has no quirks for.
static GATEWAY: Dialect = Dialect::gateway("gateway", "", "");

/// The model `route` speaks for `model`, keyed with `key`, sent through
/// `transport`.
pub(crate) fn model(
    route: &Route,
    key: &str,
    model: &str,
    transport: Transport,
) -> DynModel<Completion> {
    let base = route.base.as_str();
    match route.wire {
        Wire::Messages => {
            let client = AnthropicConfig::new(key)
                .with_base_url(base)
                .connect(transport);
            let mut messages = client.completion(model);
            messages.wire = messages
                .wire
                .with_automatic_caching()
                .with_default_max_tokens(MAX_TOKENS);
            messages.erase()
        }
        Wire::Responses => OpenAIConfig::new(key)
            .with_base_url(base)
            .connect(transport)
            .responses(model)
            .erase(),
        Wire::Chat => {
            let dialect = route
                .dialect
                .as_deref()
                .and_then(by_name)
                .unwrap_or(&GATEWAY);
            OpenAIConfig::with_key(dialect, key)
                .with_base_url(base)
                .connect(transport)
                .chat(model)
                .erase()
        }
    }
}
