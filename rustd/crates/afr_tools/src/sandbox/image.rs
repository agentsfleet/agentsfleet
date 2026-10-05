//! `image`: a file the model looks at on its next turn.
//!
//! Codex's `view_image` (`codex-rs/core/src/tools/handlers/view_image.rs`)
//! reads the file and answers the call with image content. Here the bytes
//! stay out of the call's text, which the thread and the trace keep, and wait
//! on the lease for the loop to attach to this call's result; what the model
//! and the thread read is the path, the kind and the byte count. A wire that
//! cannot be shown an image refuses the call before any read, and a file that
//! is not one of the four kinds the wires take is refused after it.

use bytes::Bytes;
use schemars::JsonSchema;
use serde::Deserialize;

use super::executor_of;
use super::files::{Answer, failed, inside, settled};
use crate::catalog::{Entry, IMAGE};
use crate::handler::Handler;
use crate::runtime::{ToolContext, ToolErrorCode, ToolOutput};

/// The most an image may weigh: the Messages wire's cap per image, the
/// stricter of the two wires that take one.
pub const IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;
/// How each kind begins.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_MAGIC: &[u8] = &[0xff, 0xd8, 0xff];
const GIF_MAGIC: &[u8] = b"GIF8";
const RIFF_MAGIC: &[u8] = b"RIFF";
/// What a RIFF container says at byte eight when it holds a WebP.
const WEBP_MAGIC: &[u8] = b"WEBP";
/// Where in a RIFF container that word sits.
const WEBP_MAGIC_AT: usize = 8;
/// What a call reads back when the model's wire takes no image.
const NO_IMAGE_INPUT: &str = "this model's wire takes no image in a tool result";

/// The kinds of image the wires take, told by their first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    /// Portable Network Graphics.
    Png,
    /// Joint Photographic Experts Group.
    Jpeg,
    /// Graphics Interchange Format.
    Gif,
    /// WebP.
    Webp,
}

impl ImageKind {
    /// The kind `bytes` begin as, if any.
    fn sniffed(bytes: &[u8]) -> Option<Self> {
        let webp = bytes.starts_with(RIFF_MAGIC)
            && bytes
                .get(WEBP_MAGIC_AT..WEBP_MAGIC_AT + WEBP_MAGIC.len())
                .is_some_and(|word| word == WEBP_MAGIC);
        if bytes.starts_with(PNG_MAGIC) {
            Some(Self::Png)
        } else if bytes.starts_with(JPEG_MAGIC) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(GIF_MAGIC) {
            Some(Self::Gif)
        } else if webp {
            Some(Self::Webp)
        } else {
            None
        }
    }

    /// The media type the kind is named by.
    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// An image a call read, waiting on the lease for the loop to attach to the
/// call's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    /// What kind of image it is.
    pub kind: ImageKind,
    /// The bytes, as the file holds them.
    pub bytes: Bytes,
}

/// `image`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Named {
    /// The image file, relative to the workspace: PNG, JPEG, GIF or WebP.
    path: String,
}

/// Shows the model a file.
#[derive(Debug)]
pub(crate) struct Image;

#[async_trait::async_trait]
impl Handler for Image {
    const ENTRY: &'static Entry = &IMAGE;
    const DESCRIPTION: &'static str = "Look at an image file in the workspace: PNG, JPEG, GIF \
        or WebP, at most 5 MiB. The image is shown to you with this call's result on your next \
        turn.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: ToolContext<'_, '_>) -> ToolOutput {
        settled(view(context, &arguments.path).await)
    }
}

async fn view(context: ToolContext<'_, '_>, path: &str) -> Answer {
    let executor = executor_of(&context)?;
    if !context.lease.image_input {
        return Err(ToolOutput::failed(
            ToolErrorCode::ImageInputUnavailable,
            NO_IMAGE_INPUT,
        ));
    }
    let path = inside(path)?;
    let fetched = executor
        .read_file(path, IMAGE_MAX_BYTES)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(ToolOutput::failed(
            ToolErrorCode::FileTooLarge,
            &format!("{path} is longer than {IMAGE_MAX_BYTES} bytes"),
        ));
    }
    let kind = ImageKind::sniffed(&fetched.data).ok_or_else(|| {
        ToolOutput::failed(
            ToolErrorCode::NotAnImage,
            &format!("{path} is not a PNG, JPEG, GIF or WebP image"),
        )
    })?;
    let bytes = fetched.data.len();
    context.lease.attachment = Some(ImageAttachment {
        kind,
        bytes: fetched.data,
    });
    Ok(ToolOutput::succeeded(format!(
        "Attached {path} ({bytes} bytes, {}) for your next turn",
        kind.mime()
    )))
}

#[cfg(test)]
#[path = "image/tests.rs"]
mod tests;
