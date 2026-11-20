//! `image`: a file the model looks at on its next step.
//!
//! Codex's `view_image` (`codex-rs/core/src/tools/handlers/view_image.rs`)
//! reads the file and answers the call with image content. Here the bytes
//! stay out of the call's text, which the thread and the trace keep, and ride
//! the output for the loop to attach to this call's result; what the model
//! and the thread read is the path, the kind and the byte count. A wire that
//! cannot be shown an image refuses the call before any read, and a file that
//! is not one of the four kinds the wires take is refused after it.

use schemars::JsonSchema;
use serde::Deserialize;

use super::{Answer, failed, inside, settled};
use crate::tool::{Image, ImageKind, ToolOutput};
use crate::tools::process::env_of;
use crate::tools::{Context, Handler, code};

/// The most an image may weigh: the Messages wire's cap per image, the
/// stricter of the two wires that take one.
pub const IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;
/// How a PNG begins.
pub const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
/// How a JPEG begins.
pub const JPEG_MAGIC: &[u8] = &[0xff, 0xd8, 0xff];
/// How a GIF begins.
const GIF_MAGIC: &[u8] = b"GIF8";
const RIFF_MAGIC: &[u8] = b"RIFF";
/// What a RIFF container says at byte eight when it holds a WebP.
const WEBP_MAGIC: &[u8] = b"WEBP";
/// Where in a RIFF container that word sits.
const WEBP_MAGIC_AT: usize = 8;
/// What a call reads back when the model's wire takes no image.
const NO_IMAGE_INPUT: &str = "this model's wire takes no image in a tool result";

/// The kind of image the wires take that `bytes` begin as, if any.
#[must_use]
pub fn sniffed(bytes: &[u8]) -> Option<ImageKind> {
    let webp = bytes.starts_with(RIFF_MAGIC)
        && bytes
            .get(WEBP_MAGIC_AT..WEBP_MAGIC_AT + WEBP_MAGIC.len())
            .is_some_and(|word| word == WEBP_MAGIC);
    if bytes.starts_with(PNG_MAGIC) {
        Some(ImageKind::Png)
    } else if bytes.starts_with(JPEG_MAGIC) {
        Some(ImageKind::Jpeg)
    } else if bytes.starts_with(GIF_MAGIC) {
        Some(ImageKind::Gif)
    } else if webp {
        Some(ImageKind::Webp)
    } else {
        None
    }
}

/// `image`'s arguments.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    /// The image file, relative to the workspace: PNG, JPEG, GIF or WebP.
    path: String,
}

/// Shows the model a file.
#[derive(Debug, Clone, Copy)]
pub struct ImageTool;

impl Handler for ImageTool {
    const NAME: &'static str = "image";
    const DESCRIPTION: &'static str = "Look at an image file in the workspace: PNG, JPEG, GIF \
        or WebP, at most 5 MiB. The image is shown to you with this call's result on your next \
        step.";
    type Arguments = Named;

    async fn run(&self, arguments: Named, context: Context<'_>) -> ToolOutput {
        settled(view(context, &arguments.path).await)
    }
}

async fn view(context: Context<'_>, path: &str) -> Answer {
    let env = env_of(&context)?;
    if !context.image_input {
        return Err(ToolOutput::failed(
            code::IMAGE_INPUT_UNAVAILABLE,
            NO_IMAGE_INPUT,
        ));
    }
    let path = inside(env, path)?;
    let fetched = env
        .read_file(path, IMAGE_MAX_BYTES)
        .await
        .map_err(|failure| failed(&failure))?;
    if fetched.truncated {
        return Err(ToolOutput::failed(
            code::FILE_TOO_LARGE,
            &format!("{path} is longer than {IMAGE_MAX_BYTES} bytes"),
        ));
    }
    let kind = sniffed(&fetched.data).ok_or_else(|| {
        ToolOutput::failed(
            code::NOT_AN_IMAGE,
            &format!("{path} is not a PNG, JPEG, GIF or WebP image"),
        )
    })?;
    let bytes = fetched.data.len();
    let attached = ToolOutput::succeeded(format!(
        "Attached {path} ({bytes} bytes, {}) for your next step",
        kind.media_type()
    ));
    Ok(attached.with_image(Image {
        kind,
        data: fetched.data,
    }))
}
