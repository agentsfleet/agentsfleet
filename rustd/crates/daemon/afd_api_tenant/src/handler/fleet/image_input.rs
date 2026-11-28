//! Validate attached bytes before handing the message to the turn ledger.

use af_core::error_code;
use af_wire::protocol::{CONTROL_MESSAGE_BYTES_MAX, MESSAGE_BYTES_MAX};
use af_wire::turn::MessageImage;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use garde::Unvalidated;
use guffbolt::{ImageKind, tools::files::image::message_image};

use crate::handler::Refusal;

// The JSON request is itself a string in the assignment frame. Keep space
// for the assignment's control data and its quoting around the request.
const REQUEST_IN_FRAME_MAX: usize = MESSAGE_BYTES_MAX - CONTROL_MESSAGE_BYTES_MAX;
const IMAGE_BASE64_MAX: usize = (5_usize * 1024 * 1024).div_ceil(3) * 4;

#[derive(garde::Validate)]
struct Encoded<'a> {
    #[garde(length(bytes, min = 1, max = IMAGE_BASE64_MAX))]
    data: &'a str,
}

#[derive(garde::Validate)]
struct Quoted {
    #[garde(range(max = REQUEST_IN_FRAME_MAX))]
    bytes: usize,
}

pub(super) async fn validate(text: &str, images: &[MessageImage]) -> Result<(), Refusal> {
    if images.is_empty() {
        return Ok(());
    }
    let request = serde_json::json!({"message": text, "images": images}).to_string();
    let bytes = af_validate::serialized_len(&request)
        .map_err(|_serialization| Refusal::malformed("Invalid message body"))?;
    Unvalidated::new(Quoted { bytes })
        .validate()
        .map_err(|_worker| {
            Refusal::coded(
                error_code::MESSAGE_IMAGE_INVALID,
                "The message and its images exceed the runner frame allowance",
            )
        })?;
    let images = images.to_vec();
    tokio::task::spawn_blocking(move || images.iter().try_for_each(check))
        .await
        .map_err(|_bounds| {
            Refusal::coded(
                error_code::INTERNAL_OPERATION_FAILED,
                "Unable to validate attached images",
            )
        })?
}

fn check(image: &MessageImage) -> Result<(), Refusal> {
    let invalid = |detail| Refusal::coded(error_code::MESSAGE_IMAGE_INVALID, detail);
    let bounded = Unvalidated::new(Encoded { data: &image.data })
        .validate()
        .map_err(|_encoded| invalid("Each image must contain 1 byte to 5 MiB of data"))?;
    let kind = ImageKind::from_media_type(&image.media_type)
        .ok_or_else(|| invalid("Attach a PNG, JPEG, GIF or WebP image"))?;
    let data = STANDARD
        .decode(bounded.data)
        .map_err(|_invalid_base64| invalid("Image data must be standard base64"))?;
    message_image(kind, data.into())
        .map(drop)
        .map_err(|failure| Refusal::coded_sentence(error_code::MESSAGE_IMAGE_INVALID, failure.text))
}
