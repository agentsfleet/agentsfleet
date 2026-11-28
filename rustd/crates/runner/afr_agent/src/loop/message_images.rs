//! Turn attachments stay in the user message and are fitted to the model wire.

use af_core::error_code;
use af_wire::{
    run::RunPayload,
    turn::{MessageImage, TurnKind, images_of},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use guffbolt::{Image, ImageKind, tools::files::image::message_image};

use crate::error::{Error, ErrorKind, Result};

pub(super) fn current(run: &RunPayload<'_>, image_input: bool) -> Result<Vec<Image>> {
    if run.event.event_type != TurnKind::Chat {
        return Ok(Vec::new());
    }
    let images = images_of(&run.event.request_json);
    if !image_input && !images.is_empty() {
        return Err(ErrorKind::MessageImage(error_code::MESSAGE_IMAGE_UNSUPPORTED).into());
    }
    decode(&images)
}

pub(super) fn decode(images: &[MessageImage]) -> Result<Vec<Image>> {
    let invalid = || Error::from(ErrorKind::MessageImage(error_code::MESSAGE_IMAGE_INVALID));
    images
        .iter()
        .map(|image| {
            let kind = ImageKind::from_media_type(&image.media_type).ok_or_else(invalid)?;
            let bytes = STANDARD.decode(&image.data).map_err(|_encoded| invalid())?;
            message_image(kind, bytes.into()).map_err(|_refused| invalid())
        })
        .collect()
}
