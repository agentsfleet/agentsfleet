//! An image a tool hands the model on the turn its result opens.
//!
//! The seam carries the bytes and their kind; the wire spells them. rig maps
//! one tool-result image onto a Messages `image` block and a Responses
//! `input_image`, each from the same base64 content, so the two wires that
//! take an image take it from here (`rig-core/src/providers/anthropic/
//! completion.rs`, `openai/responses_api/mod.rs`). Chat completions take text
//! alone, which [`Wire::carries_images`](crate::Wire::carries_images) says.

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use rig_core::message::{DocumentSourceKind, Image, ImageMediaType, ToolResultContent};

/// The kinds of image the wires take.
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
    /// The media type rig names the kind by, and the wires after it.
    const fn media_type(self) -> ImageMediaType {
        match self {
            Self::Png => ImageMediaType::PNG,
            Self::Jpeg => ImageMediaType::JPEG,
            Self::Gif => ImageMediaType::GIF,
            Self::Webp => ImageMediaType::WEBP,
        }
    }
}

/// An image a call read, for the model to see with the call's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInput {
    /// What kind of image it is.
    pub kind: ImageKind,
    /// The bytes, as the file holds them.
    pub data: Bytes,
}

/// `image` as rig carries it inside a tool result: base64, under its media
/// type, with no detail hint, so each wire applies its own default.
pub(crate) fn content(image: &ImageInput) -> ToolResultContent {
    ToolResultContent::Image(Image {
        data: DocumentSourceKind::Base64(BASE64_STANDARD.encode(&image.data)),
        media_type: Some(image.kind.media_type()),
        ..Image::default()
    })
}
