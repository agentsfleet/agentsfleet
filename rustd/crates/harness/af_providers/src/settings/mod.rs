//! What a saved model setting means to agentsfleet.
//!
//! The route a `ModelInference` names, the endpoint rules it is proved
//! against, and the startup timeout and retry settings every run's provider
//! is built with.
//!
//! The daemon reads this module to validate what an admin saves and what a
//! credential may reach; it never reaches a provider through it. Connecting
//! is [`Connect`](crate::Connect)'s, in the runner.

mod endpoint;
mod timeouts;

pub use self::endpoint::{provider_endpoint, validate_decision};
pub use self::timeouts::{CONNECT_TIMEOUT, TimeoutSettings, Timeouts};
pub use crate::retry::settings::{RetryPolicy, RetrySettings};

use af_wire::inference::{InferenceApi, ModelInference};
use garde::Validate as _;
use guffbolt_ai::wire::{Api, Route};
use tracing_subscriber::filter::{LevelFilter, Targets};

use crate::error::{ErrorKind, Result};

/// A validated model route: saved settings proved against their bounds and
/// endpoint rules, then the route they name.
#[derive(Debug, Clone)]
pub struct Configured {
    base_url: String,
    route: Route,
}

impl Configured {
    /// Proves bounds, endpoint shape, API dialect and model output/context consistency.
    ///
    /// # Errors
    /// Refuses invalid settings; callers additionally enforce host and credential ownership.
    pub fn new(settings: &ModelInference, context_tokens: u32) -> Result<Self> {
        settings.validate()?;
        provider_endpoint(&settings.base_url)?;
        let route = Route::new(
            api(settings.api),
            &settings.base_url,
            settings.dialect.as_deref(),
            settings.max_output_tokens,
            context_tokens,
        )
        .map_err(|source| ErrorKind::Unroutable { source })?;
        Ok(Self {
            base_url: settings.base_url.clone(),
            route,
        })
    }

    /// The validated host, in the URL spelling used by the egress allowlist.
    #[must_use]
    pub fn into_host(self) -> Box<str> {
        self.route.host().into()
    }

    /// The route the settings name.
    #[must_use]
    pub fn into_route(self) -> Route {
        self.route
    }

    /// The API the settings name, as the wires spell it.
    #[must_use]
    pub fn api(&self) -> Api {
        self.route.api()
    }

    /// A credential pinned to an endpoint can authorize only that persisted route.
    #[must_use]
    pub fn accepts_endpoint(&self, endpoint: Option<&str>) -> bool {
        endpoint.is_none_or(|endpoint| {
            endpoint.trim_end_matches('/') == self.base_url.trim_end_matches('/')
        })
    }

    /// Whether the selected SDK wire can carry tool calls for this model.
    #[must_use]
    pub fn supports_tools(&self, model: &str) -> bool {
        self.route.supports_tools(model)
    }
}

/// The API `api` names, as the wires spell it: the one place a saved API
/// becomes a wire.
const fn api(api: InferenceApi) -> Api {
    match api {
        InferenceApi::Chat => Api::Chat,
        InferenceApi::Responses => Api::Responses,
        InferenceApi::Messages => Api::Messages,
        InferenceApi::Gemini => Api::Gemini,
        InferenceApi::Ollama => Api::Ollama,
    }
}

/// The filter daemon and runner install for `level`: everything at `level`,
/// with rig held to its warnings and its quoting modules off, so no level puts
/// a model's raw reply in the journal.
#[must_use]
pub fn log_filter(level: LevelFilter) -> Targets {
    guffbolt_ai::log_filter(level)
}

#[cfg(test)]
mod tests;
