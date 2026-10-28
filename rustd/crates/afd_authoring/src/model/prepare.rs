//! Validate the complete assembled request with its selected tokenizer before intent.

use super::ModelCallInput;
#[cfg(test)]
use super::provider;
#[cfg(test)]
use crate::error::ErrorKind;
use crate::{Result, error::raise::stored};
#[cfg(test)]
use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelMessage};

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    bytes: &'a [u8],
}

pub(super) struct Prepared {
    pub(super) body: rig_core::completion::CompletionRequest,
    pub(super) output_limit: u32,
    /// The route the body was validated against, so the dispatch sends on
    /// exactly that route.
    pub(super) inference: afd_wire::inference::ModelInference,
    /// The context window the body was measured against.
    pub(super) context_tokens: u32,
    pub(super) compaction: Option<afd_context::Compaction>,
    pub(super) input_tokens: u64,
    pub(super) measurement: Option<afd_inference::InputCount>,
    pub(super) context_digest: String,
}

pub(super) fn request(mut request: ModelCallInput) -> Result<garde::Valid<ModelCallInput>> {
    let original = serde_json::to_vec(&request).map_err(stored)?;
    garde::Unvalidated::new(Envelope { bytes: &original }).validate()?;
    let _bounds = garde::Unvalidated::new(request.clone()).validate()?;
    for message in &mut request.request.messages {
        let text = match message {
            ModelMessage::User { text }
            | ModelMessage::Assistant { text, .. }
            | ModelMessage::Tool { text, .. } => text,
        };
        *text = crate::message::normalize(text);
    }
    garde::Unvalidated::new(request)
        .validate()
        .map_err(Into::into)
}

#[cfg(test)]
pub(super) fn body(
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
    continuations: Vec<super::continuation::Continuation>,
    editing: bool,
) -> Result<Prepared> {
    let settings = model.inference.as_ref().ok_or(ErrorKind::Model)?;
    let context =
        u32::try_from(model.context_window_tokens).map_err(|_invalid| ErrorKind::Model)?;
    let route = afd_inference::Configured::new(settings, context)?;
    let mut canonical = provider::body(request, model, editing)?;
    super::continuation::restore(&mut canonical, request, continuations)?;
    // Saved continuation indexes address conversation messages without the system prefix.
    canonical = canonical.preamble(provider::system(editing, false));
    let input = input_tokens(&route, &canonical, request, model)?;
    if input >= i64::from(context) {
        return Err(ErrorKind::Token.into());
    }
    let remaining =
        u32::try_from(i64::from(context) - input).map_err(|_overflow| ErrorKind::Token)?;
    let output_limit = request.request.max_output_tokens.min(remaining);
    canonical.max_tokens = Some(u64::from(output_limit));
    Ok(Prepared {
        body: canonical,
        output_limit,
        inference: settings.clone(),
        context_tokens: context,
        compaction: None,
        input_tokens: input.cast_unsigned(),
        measurement: None,
        context_digest: String::new(),
    })
}

#[cfg(test)]
fn input_tokens(
    route: &afd_inference::Configured,
    canonical: &rig_core::completion::CompletionRequest,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
) -> Result<i64> {
    let _ = request;
    i64::try_from(route.measure_input(&model.model, canonical)?)
        .map_err(|_oversized| ErrorKind::Token.into())
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
