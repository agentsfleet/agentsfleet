//! The persisted inference settings agentsfleet validates, over the route
//! `guffbolt_ai::wire` speaks; authorization and key resolution stay with
//! callers.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod endpoint;
mod error;
mod timeouts;
pub mod transport;

pub use endpoint::{provider_endpoint, validate_decision};
pub use error::{Error, Result};
pub use guffbolt_ai::log_filter;
pub use guffbolt_ai::wire::input_tokens;
pub use timeouts::{CONNECT_TIMEOUT, TimeoutSettings, Timeouts};

use af_wire::inference::{InferenceApi, ModelInference};
use garde::Validate as _;
use guffbolt_ai::wire::{Api, Route};

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
        )?;
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

/// The API `api` names, as the wires spell it.
#[must_use]
pub const fn api(api: InferenceApi) -> Api {
    match api {
        InferenceApi::Chat => Api::Chat,
        InferenceApi::Responses => Api::Responses,
        InferenceApi::Messages => Api::Messages,
        InferenceApi::Gemini => Api::Gemini,
        InferenceApi::Ollama => Api::Ollama,
    }
}

#[cfg(test)]
mod tests;
