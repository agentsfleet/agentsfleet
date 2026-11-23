//! One model's route: the API it speaks, where, in which dialect, and the
//! limits its turns are bounded by. No provider-name or model-name routing
//! heuristics: every choice is the caller's, named explicitly.

use rig_core::completion::CompletionRequest;
use rig_core::providers::{anthropic, gemini, ollama, openai};
use rig_core::wire::{Encoded, Mode, Operation as _};
use rig_core::{
    DynModel,
    http_client::{DynHttpClient, HttpClientExt},
    operation::Completion,
};
use serde::Serialize;

use crate::error::{ErrorKind, Result};

static GATEWAY: openai::wire::Dialect = openai::wire::Dialect::gateway("gateway", "", "");

/// The API a model is reached through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Api {
    /// OpenAI-compatible chat completions.
    Chat,
    /// `OpenAI` Responses.
    Responses,
    /// Anthropic Messages.
    Messages,
    /// Google's native `GenerateContent` API.
    Gemini,
    /// Ollama's native chat API.
    Ollama,
}

impl Api {
    /// Whether a tool's images reach the model. Messages, Responses and
    /// Gemini 3 read them inside the tool result; Ollama, and Gemini before
    /// 3, refuse them there (rig-core 0.44 `providers/ollama/chat.rs:298`,
    /// `gemini/completion.rs:333`), and rig's history adapter moves them into
    /// the user message that follows (`completion/history.rs:1205`), which
    /// both read. Chat completions take text alone.
    #[must_use]
    pub const fn carries_images(self) -> bool {
        matches!(
            self,
            Self::Messages | Self::Responses | Self::Gemini | Self::Ollama
        )
    }
}

/// A checked model route: its API, base URL, rig dialect, output limit and
/// context window.
#[derive(Debug, Clone)]
pub struct Route {
    settings: Settings,
    host: Box<str>,
    dialect: Dialect,
    context_tokens: u32,
}

/// What the route was built from, in the form its digests are taken over.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Settings {
    pub(crate) api: Api,
    pub(crate) base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) dialect: Option<String>,
    pub(crate) max_output_tokens: u32,
}

#[derive(Debug, Clone)]
enum Dialect {
    Chat(&'static openai::wire::Dialect),
    Responses(&'static openai::wire::Dialect),
    Messages(anthropic::wire::Dialect),
    Gemini,
    Ollama,
}

impl Route {
    /// A route over `api` at `base_url`, in rig's `dialect` of that API or
    /// its default, writing at most `max_output_tokens` of a
    /// `context_tokens` window.
    ///
    /// # Errors
    /// The base URL does not parse or names no host, the output limit exceeds
    /// the window, or the dialect is not one the API has.
    pub fn new(
        api: Api,
        base_url: &str,
        dialect: Option<&str>,
        max_output_tokens: u32,
        context_tokens: u32,
    ) -> Result<Self> {
        let url = url::Url::parse(base_url)?;
        let host = url.host_str().ok_or(ErrorKind::Endpoint)?;
        if max_output_tokens > context_tokens {
            return Err(ErrorKind::OutputLimit.into());
        }
        let wire = match api {
            Api::Chat | Api::Responses => {
                let named = dialect
                    .map_or(Some(&GATEWAY), openai::wire::by_name)
                    .ok_or(ErrorKind::Dialect)?;
                if api == Api::Chat {
                    Dialect::Chat(named)
                } else {
                    Dialect::Responses(named)
                }
            }
            Api::Messages => Dialect::Messages(
                dialect
                    .map_or(
                        Some(anthropic::wire::ANTHROPIC),
                        anthropic::wire::Dialect::by_name,
                    )
                    .ok_or(ErrorKind::Dialect)?,
            ),
            // The native APIs have no dialect table: a named one is a mistake
            // the caller must hear about rather than one silently ignored.
            Api::Gemini | Api::Ollama if dialect.is_some() => {
                return Err(ErrorKind::Dialect.into());
            }
            Api::Gemini => Dialect::Gemini,
            Api::Ollama => Dialect::Ollama,
        };
        Ok(Self {
            settings: Settings {
                api,
                base_url: base_url.to_owned(),
                dialect: dialect.map(str::to_owned),
                max_output_tokens,
            },
            host: host.into(),
            dialect: wire,
            context_tokens,
        })
    }

    /// The API the route speaks.
    #[must_use]
    pub const fn api(&self) -> Api {
        self.settings.api
    }

    /// The host the base URL names, in its URL spelling.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The base URL every request is sent under.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.settings.base_url
    }

    /// The most one turn may write.
    #[must_use]
    pub const fn max_output_tokens(&self) -> u32 {
        self.settings.max_output_tokens
    }

    /// The model's context window.
    #[must_use]
    pub const fn context_tokens(&self) -> u32 {
        self.context_tokens
    }

    pub(crate) const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether the selected wire can carry tool calls for `model`.
    #[must_use]
    pub fn supports_tools(&self, model: &str) -> bool {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                tools(&self.openai(dialect, model, ""), model)
            }
            Dialect::Messages(dialect) => tools(&self.messages(dialect, model, ""), model),
            Dialect::Gemini => tools(&self.gemini(model, ""), model),
            Dialect::Ollama => tools(&self.ollama(model, ""), model),
        }
    }

    /// A rig model for `model` with `key`, over `transport`.
    #[must_use]
    pub(crate) fn model(
        &self,
        model: &str,
        key: &str,
        transport: impl HttpClientExt + 'static,
    ) -> DynModel<Completion> {
        self.with_transport(model, key, DynHttpClient::new(transport))
    }

    /// A rig model over a caller-owned physical transport, without rig's
    /// own retries.
    pub(crate) fn with_transport<T>(
        &self,
        model: &str,
        key: &str,
        transport: T,
    ) -> DynModel<Completion>
    where
        T: rig_core::driver::Transport<openai::wire::OpenAiWire>
            + rig_core::driver::Transport<anthropic::wire::Messages>
            + rig_core::driver::Transport<gemini::completion::GenerateContent>
            + rig_core::driver::Transport<ollama::Chat>,
    {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                rig_core::Model::new(self.openai(dialect, model, key), transport).erase()
            }
            Dialect::Messages(dialect) => {
                rig_core::Model::new(self.messages(dialect, model, key), transport).erase()
            }
            Dialect::Gemini => rig_core::Model::new(self.gemini(model, key), transport).erase(),
            Dialect::Ollama => rig_core::Model::new(self.ollama(model, key), transport).erase(),
        }
    }

    /// Encodes without credentials, so size and token bounds are proved
    /// before anything is sent.
    ///
    /// # Errors
    /// Preserves rig's request-shape and encoding failures.
    #[cfg(test)]
    pub(crate) fn encode(&self, model: &str, request: CompletionRequest) -> Result<Encoded> {
        self.encode_key(model, "", request, Mode::Unary)
    }

    /// Encodes the exact streaming request before it is sent.
    ///
    /// # Errors
    /// Preserves rig's request-shape and encoding failures.
    pub(crate) fn encode_stream(&self, model: &str, request: CompletionRequest) -> Result<Encoded> {
        self.encode_key(model, "", request, Mode::Streaming)
    }

    pub(crate) fn encode_key(
        &self,
        model: &str,
        key: &str,
        request: CompletionRequest,
        mode: Mode,
    ) -> Result<Encoded> {
        match self.dialect {
            Dialect::Chat(dialect) | Dialect::Responses(dialect) => {
                encode(&self.openai(dialect, model, key), request, mode)
            }
            Dialect::Messages(dialect) => {
                encode(&self.messages(dialect, model, key), request, mode)
            }
            Dialect::Gemini => encode(&self.gemini(model, key), request, mode),
            Dialect::Ollama => encode(&self.ollama(model, key), request, mode),
        }
    }

    fn gemini(&self, model: &str, key: &str) -> gemini::completion::GenerateContent {
        let provider = gemini::GeminiConfig::new(key).with_base_url(&self.settings.base_url);
        gemini::completion::GenerateContent::new(provider, model)
    }

    fn ollama(&self, model: &str, key: &str) -> ollama::Chat {
        let provider = ollama::wire::OllamaConfig::new()
            .with_base_url(&self.settings.base_url)
            .with_api_key(key);
        ollama::Chat::new(provider, model)
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
            facts: rig_core::catalog::ModelFacts::default(),
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
