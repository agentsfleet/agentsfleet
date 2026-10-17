//! Rig codecs for explicit persisted settings; authorization and key resolution stay with callers.

mod budget;
mod error;
mod json;
mod logs;

pub use budget::input_tokens;
pub use json::json_request;
pub use logs::log_filter;

use afd_wire::inference::{InferenceApi, ModelInference};
use garde::Validate as _;
use rig_core::completion::CompletionRequest;
use rig_core::providers::{anthropic, openai};
use rig_core::wire::{Encoded, Mode, Operation as _};
use rig_core::{
    DynModel,
    http_client::{DynHttpClient, HttpClientExt},
    operation::Completion,
};

use error::ErrorKind;
pub use error::{Error, Result};

static GATEWAY: openai::wire::Dialect = openai::wire::Dialect::gateway("gateway", "", "");

/// A validated, borrowed model route. No provider-name or model-name routing heuristics.
#[derive(Debug)]
pub struct Configured<'a> {
    settings: &'a ModelInference,
    host: Box<str>,
    dialect: Dialect,
    context_tokens: u32,
}

#[derive(Debug)]
enum Dialect {
    Chat(&'static openai::wire::Dialect),
    Responses(&'static openai::wire::Dialect),
    Messages(anthropic::wire::Dialect),
}

impl<'a> Configured<'a> {
    /// Proves bounds, endpoint shape, API dialect and model output/context consistency.
    ///
    /// # Errors
    /// Refuses invalid settings; callers additionally enforce host and credential ownership.
    pub fn new(settings: &'a ModelInference, context_tokens: u32) -> Result<Self> {
        settings.validate()?;
        let url = url::Url::parse(&settings.base_url)?;
        let host = url.host_str().ok_or(ErrorKind::Endpoint)?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ErrorKind::Endpoint.into());
        }
        let blocked = match url.host() {
            Some(url::Host::Ipv4(address)) => afd_core::net::is_blocked(address.into()),
            Some(url::Host::Ipv6(address)) => afd_core::net::is_blocked(address.into()),
            _ => false,
        };
        if blocked {
            return Err(ErrorKind::Endpoint.into());
        }
        if settings.max_output_tokens > context_tokens {
            return Err(ErrorKind::OutputLimit.into());
        }
        let dialect = match settings.api {
            InferenceApi::Chat | InferenceApi::Responses => {
                let dialect = settings
                    .dialect
                    .as_deref()
                    .map_or(Some(&GATEWAY), openai::wire::by_name)
                    .ok_or(ErrorKind::Dialect)?;
                if settings.api == InferenceApi::Chat {
                    Dialect::Chat(dialect)
                } else {
                    Dialect::Responses(dialect)
                }
            }
            InferenceApi::Messages => Dialect::Messages(
                settings
                    .dialect
                    .as_deref()
                    .map_or(
                        Some(anthropic::wire::ANTHROPIC),
                        anthropic::wire::Dialect::by_name,
                    )
                    .ok_or(ErrorKind::Dialect)?,
            ),
        };
        Ok(Self {
            settings,
            host: host.into(),
            dialect,
            context_tokens,
        })
    }

    /// The validated host, in the URL spelling used by the egress allowlist.
    #[must_use]
    pub fn into_host(self) -> Box<str> {
        self.host
    }

    /// Creates a rig model using the caller's guarded transport and secret.
    #[must_use]
    pub fn model(
        &self,
        model: &str,
        key: &str,
        transport: impl HttpClientExt + 'static,
    ) -> DynModel<Completion> {
        self.with_transport(model, key, DynHttpClient::new(transport))
    }

    /// A credential pinned to an endpoint can authorize only that persisted route.
    #[must_use]
    pub fn accepts_endpoint(&self, endpoint: Option<&str>) -> bool {
        endpoint.is_none_or(|endpoint| {
            endpoint.trim_end_matches('/') == self.settings.base_url.trim_end_matches('/')
        })
    }

    /// Whether the selected SDK wire can carry tool calls for this model.
    #[must_use]
    pub fn supports_tools(&self, model: &str) -> bool {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                tools(&self.openai(dialect, model, ""), model)
            }
            Dialect::Messages(dialect) => tools(&self.messages(dialect, model, ""), model),
        }
    }

    /// Creates a model over a caller-owned physical transport without SDK retries.
    #[must_use]
    pub fn with_transport<T>(&self, model: &str, key: &str, transport: T) -> DynModel<Completion>
    where
        T: rig_core::driver::Transport<openai::wire::OpenAiWire>
            + rig_core::driver::Transport<anthropic::wire::Messages>,
    {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                rig_core::Model::new(self.openai(dialect, model, key), transport).erase()
            }
            Dialect::Messages(dialect) => {
                rig_core::Model::new(self.messages(dialect, model, key), transport).erase()
            }
        }
    }

    /// Encodes without credentials so size and token bounds are proved before durable intent.
    ///
    /// # Errors
    /// Preserves SDK request-shape and encoding failures.
    pub fn encode(&self, model: &str, request: CompletionRequest) -> Result<Encoded> {
        self.encode_mode(model, request, Mode::Unary)
    }

    fn encode_mode(&self, model: &str, request: CompletionRequest, mode: Mode) -> Result<Encoded> {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                encode(&self.openai(dialect, model, ""), request, mode)
            }
            Dialect::Messages(dialect) => encode(&self.messages(dialect, model, ""), request, mode),
        }
    }

    fn openai(
        &self,
        dialect: &'static openai::wire::Dialect,
        model: &str,
        key: &str,
    ) -> openai::wire::OpenAiWire {
        let route = if matches!(self.dialect, Dialect::Chat(_)) {
            openai::wire::Route::Chat
        } else {
            openai::wire::Route::Responses
        };
        let provider = openai::OpenAIConfig::with_key(dialect, key)
            .with_base_url(&self.settings.base_url)
            .with_route(route);
        openai::wire::OpenAiWire::new(provider, model)
    }

    fn messages(
        &self,
        dialect: anthropic::wire::Dialect,
        model: &str,
        key: &str,
    ) -> anthropic::wire::Messages {
        anthropic::wire::Messages {
            provider: anthropic::AnthropicConfig::with_key(&dialect, key)
                .with_base_url(&self.settings.base_url),
            model: model.into(),
            default_max_tokens: Some(u64::from(self.settings.max_output_tokens)),
            prompt_caching: true,
            static_prefix_cache_ttl: None,
            strict_tools: false,
            tool_input_streaming: dialect.quirks.tool_input_streaming,
        }
    }
}

fn tools(wire: &impl rig_core::wire::Wire, model: &str) -> bool {
    wire.describe()
        .replay
        .is_some_and(|target| target.accepts(model).tools)
}

fn encode<W>(wire: &W, request: CompletionRequest, mode: Mode) -> Result<Encoded>
where
    W: rig_core::wire::Wire<Op = Completion, Payload = Encoded>,
{
    let request = Completion::prepare(request, &wire.describe())?;
    Ok(wire.encode(request, mode)?)
}

#[cfg(test)]
mod tests;
