//! Validate the complete assembled request with its selected tokenizer before intent.

use super::{ModelCallInput, budget, provider};
use crate::{
    Result,
    error::{ErrorKind, raise::stored},
};
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

pub(super) fn body(
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
    continuations: Vec<super::continuation::Continuation>,
) -> Result<Prepared> {
    let settings = model.inference.as_ref().ok_or(ErrorKind::Model)?;
    if request.request.max_output_tokens > settings.max_output_tokens {
        return Err(ErrorKind::Token.into());
    }
    let context =
        u32::try_from(model.context_window_tokens).map_err(|_invalid| ErrorKind::Model)?;
    let route = afd_inference::Configured::new(settings, context)?;
    let mut canonical = provider::body(request, model)?;
    super::continuation::restore(&mut canonical, request, continuations)?;
    let input = input_tokens(&route, &canonical, request, model)?;
    if input > budget::INPUT_PER_CALL || input >= i64::from(context) {
        return Err(ErrorKind::Token.into());
    }
    let remaining =
        u32::try_from(i64::from(context) - input).map_err(|_overflow| ErrorKind::Token)?;
    let output_limit = request.request.max_output_tokens.min(remaining);
    canonical.max_tokens = Some(u64::from(output_limit));
    Ok(Prepared {
        body: canonical,
        output_limit,
    })
}

fn input_tokens(
    route: &afd_inference::Configured,
    canonical: &rig_core::completion::CompletionRequest,
    request: &garde::Valid<ModelCallInput>,
    model: &RegisteredModel,
) -> Result<i64> {
    let encoded = route.encode(&model.model, canonical.clone())?;
    let rig_core::wire::Body::Bytes(body) = encoded.request.into_body() else {
        return Err(ErrorKind::Model.into());
    };
    garde::Unvalidated::new(Envelope { bytes: &body }).validate()?;
    let text = std::str::from_utf8(&body).map_err(|_unreachable| ErrorKind::Token)?;
    // Unknown tokenizers use a conservative UTF-8 byte bound; billing always uses reported usage.
    let tokens = super::input_tokens(&model.model, text);
    // JSON counts include schemas and escaped content. Reserve additional
    // per-item framing rather than interpreting bytes as tokens.
    let framing = request
        .request
        .messages
        .len()
        .checked_mul(32)
        .ok_or(ErrorKind::Token)?;
    let counted = tokens.checked_add(framing).ok_or(ErrorKind::Token)?;
    i64::try_from(counted).map_err(|_overflow| ErrorKind::Token.into())
}
