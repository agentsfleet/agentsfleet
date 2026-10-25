//! Bound each physical call after the SDK assembles the complete input.

use rig_core::completion::CompletionRequest;

use super::{Configured, Result, error::ErrorKind};

/// Unknown tokenizers estimate one token per UTF-8 byte; billing uses actual usage.
#[must_use]
pub fn input_tokens(model: &str, text: &str) -> usize {
    tiktoken_rs::bpe_for_model(model)
        .map_or(text.len(), |encoding| encoding.encode_ordinary(text).len())
}

impl Configured<'_> {
    /// Estimate the complete streaming request, including schemas and opaque replay.
    ///
    /// # Errors
    /// The selected wire cannot encode the request.
    pub fn measure_input(&self, model: &str, request: &CompletionRequest) -> Result<u64> {
        let encoded = self.encode_stream(model, request.clone())?;
        let body = crate::json_request(encoded.request)?.into_body();
        let tokens =
            std::str::from_utf8(&body).map_or(body.len(), |text| input_tokens(model, text));
        Ok(tokens.saturating_add(request.chat_history.len().saturating_mul(32)) as u64)
    }

    /// Reserves input space before setting the physical call's output allowance.
    ///
    /// # Errors
    /// The SDK cannot encode the request or the complete input exhausts context.
    pub fn bound_output(
        &self,
        model: &str,
        mut request: CompletionRequest,
    ) -> Result<CompletionRequest> {
        let input = self.measure_input(model, &request)?;
        let remaining = u64::from(self.context_tokens).saturating_sub(input);
        if remaining == 0 {
            return Err(ErrorKind::ContextLimit.into());
        }
        let output = u64::from(self.settings.max_output_tokens)
            .min(request.max_tokens.unwrap_or(u64::MAX))
            .min(remaining);
        request.max_tokens = Some(output);
        Ok(request)
    }
}
