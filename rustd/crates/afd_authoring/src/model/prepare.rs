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
) -> Result<Vec<u8>> {
    let encoding =
        tiktoken_rs::bpe_for_model(&model.model).map_err(|_unrecognized| ErrorKind::Model)?;
    if encoding.encode_ordinary(provider::SYSTEM).len() > 2048
        || encoding
            .encode_ordinary(&provider::tool().to_string())
            .len()
            > 4096
    {
        return Err(ErrorKind::Model.into());
    }
    let body = serde_json::to_vec(&provider::body(request, model)?).map_err(stored)?;
    garde::Unvalidated::new(Envelope { bytes: &body }).validate()?;
    let tokens = encoding
        .encode_ordinary(std::str::from_utf8(&body).map_err(|_unreachable| ErrorKind::Token)?)
        .len();
    // JSON counts include schemas and escaped content. Reserve additional
    // per-item framing rather than interpreting bytes as tokens.
    let framing = request
        .request
        .messages
        .len()
        .checked_mul(32)
        .ok_or(ErrorKind::Token)?;
    let counted = tokens.checked_add(framing).ok_or(ErrorKind::Token)?;
    let input = i64::try_from(counted).map_err(|_overflow| ErrorKind::Token)?;
    if input > budget::INPUT_PER_CALL
        || input + i64::from(request.request.max_output_tokens)
            > i64::from(model.context_window_tokens)
    {
        return Err(ErrorKind::Token.into());
    }
    Ok(body)
}
