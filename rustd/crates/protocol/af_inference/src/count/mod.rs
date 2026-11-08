//! Request-bound measurements; local tokenization remains an estimate of provider framing.

mod messages;
mod native;

use rig_core::completion::{CompletionRequest, Usage};
use sha2::{Digest as _, Sha256};

use crate::{Configured, Result, error::ErrorKind, stream::TurnControl, transport::HttpTransport};

const REVISION: &str = "rig-0.45.0:tiktoken-rs-0.12.1:request-v3";
const CONFIRM_PERCENT: u64 = 80;
const MESSAGES: &str = "messages";

/// A measurement is reusable only for its exact selected request and route.
pub struct InputCount {
    tokens: u64,
    method: &'static str,
    binding: [u8; 32],
    route: String,
    model: String,
    api: &'static str,
    kind: &'static str,
    /// How long the local measurement took. Logged because a model without a
    /// library tokenizer is counted with `o200k_base`: a full encoding pass,
    /// not a length read.
    measure_us: u64,
}

impl std::fmt::Debug for InputCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputCount")
            .field("tokens", &self.tokens)
            .field("method", &self.method)
            .field("revision", &REVISION)
            .finish_non_exhaustive()
    }
}

impl InputCount {
    /// Input allowance measured before this physical attempt.
    #[must_use]
    pub const fn tokens(&self) -> u64 {
        self.tokens
    }

    /// Distinguishes provider confirmation from local approximations.
    #[must_use]
    pub const fn method(&self) -> &'static str {
        self.method
    }

    pub(crate) fn observe(&self, usage: &Usage, correlation_id: &str) {
        if let Some(actual_input_tokens) = usage.input_tokens {
            let measured_input_tokens = self.tokens;
            let method = self.method;
            let route_id = self.route.as_str();
            let model_id = self.model.as_str();
            let provider_api = self.api;
            let request_kind = self.kind;
            let measure_us = self.measure_us;
            tracing::info!(
                event = "model_input_calibration",
                correlation_id,
                route_id,
                model_id,
                provider_api,
                request_kind,
                method,
                revision = REVISION,
                measured_input_tokens,
                actual_input_tokens,
                measure_us
            );
        }
    }
}

impl Configured<'_> {
    /// Measures the complete selected wire, without a network request or input cache.
    ///
    /// # Errors
    /// The provider wire cannot encode this request.
    pub fn input_count(&self, model: &str, request: &CompletionRequest) -> Result<InputCount> {
        let started = std::time::Instant::now();
        let tokens = self.measure_input(model, request)?;
        let measure_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        Ok(InputCount {
            tokens,
            measure_us,
            method: if tiktoken_rs::bpe_for_model(model).is_ok() {
                "tokenizer_estimate"
            } else {
                "o200k_proxy_estimate"
            },
            binding: self.count_binding(model, request)?,
            route: hex::encode(Sha256::digest(serde_json::to_vec(&(
                self.settings.as_ref(),
                model,
                self.context_tokens,
                REVISION,
            ))?)),
            model: hex::encode(Sha256::digest(model.as_bytes())),
            api: match self.settings.api {
                af_wire::inference::InferenceApi::Chat => "chat",
                af_wire::inference::InferenceApi::Responses => "responses",
                af_wire::inference::InferenceApi::Messages => MESSAGES,
                af_wire::inference::InferenceApi::Gemini => "gemini",
                af_wire::inference::InferenceApi::Ollama => "ollama",
            },
            kind: if request.tools.is_empty() {
                MESSAGES
            } else {
                "tools"
            },
        })
    }

    /// Native counting is restricted to documented APIs on the selected origin, near capacity.
    ///
    /// `input` is `request`'s local measurement, already taken by the caller:
    /// with the `o200k_base` estimate a measurement is a full encoding pass, so
    /// none is repeated here.
    #[must_use]
    pub fn needs_native_count(&self, input: u64, request: &CompletionRequest) -> bool {
        let output = request
            .max_tokens
            .unwrap_or_else(|| u64::from(self.settings.max_output_tokens));
        native::supported(self)
            && input.saturating_add(output).saturating_mul(100)
                >= u64::from(self.context_tokens).saturating_mul(CONFIRM_PERCENT)
    }

    /// Confirms near capacity through the caller's guarded transport and authorized credential.
    /// No response generation or cross-provider fallback occurs.
    ///
    /// # Errors
    /// Count failure, cancellation or deadline aborts admission rather than inventing a count.
    pub async fn confirm_input(
        &self,
        model: &str,
        key: &str,
        request: &CompletionRequest,
        transport: &HttpTransport,
        control: &TurnControl,
    ) -> Result<InputCount> {
        // The local count just taken decides, rather than a second encoding
        // pass over the same request.
        let mut count = self.input_count(model, request)?;
        if self.needs_native_count(count.tokens, request) {
            count.tokens = native::count(self, model, key, request, transport, control).await?;
            count.method = "provider_count";
        }
        Ok(count)
    }

    /// Clips output using a fresh count, then binds the count to the resulting request.
    ///
    /// # Errors
    /// A changed request/route invalidates the count; exhausted context refuses the call.
    pub fn bound_counted_output(
        &self,
        model: &str,
        mut request: CompletionRequest,
        count: &mut InputCount,
    ) -> Result<CompletionRequest> {
        self.verify_count(model, &request, count)?;
        let remaining = u64::from(self.context_tokens).saturating_sub(count.tokens);
        if remaining == 0 {
            return Err(ErrorKind::ContextLimit.into());
        }
        request.max_tokens = Some(
            u64::from(self.settings.max_output_tokens)
                .min(request.max_tokens.unwrap_or(u64::MAX))
                .min(remaining),
        );
        count.binding = self.count_binding(model, &request)?;
        Ok(request)
    }

    pub(crate) fn verify_count(
        &self,
        model: &str,
        request: &CompletionRequest,
        count: &InputCount,
    ) -> Result<()> {
        if self.count_binding(model, request)? != count.binding {
            return Err(ErrorKind::CountStale.into());
        }
        Ok(())
    }

    fn count_binding(&self, model: &str, request: &CompletionRequest) -> Result<[u8; 32]> {
        let encoded = self.encode_stream(model, request.clone())?;
        let body = crate::json_request(encoded.request)?.into_body();
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(&(
            self.settings.as_ref(),
            model,
            self.context_tokens,
            REVISION,
        ))?);
        digest.update(body);
        Ok(digest.finalize().into())
    }
}
