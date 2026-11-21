//! A tool as the model is offered it, and an image a call hands the model.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One tool as the model is offered it: provider-neutral, so each wire maps
/// it onto its own function-calling shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec<'a> {
    /// The name the model calls it by.
    pub name: &'a str,
    /// What the tool does, as the model reads it.
    pub description: &'a str,
    /// The arguments' JSON Schema.
    pub parameters: &'a Value,
}

/// The kinds of image a call can hand the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// The media type the kind is named by.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// An image a call read, for the model to see with that call's result.
///
/// It serializes with the conversation that carries it, so a resumed loop
/// hands the model the image it saw before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Image {
    /// What kind of image it is.
    pub kind: ImageKind,
    /// The bytes: the file's own, or the picture written again once a tool
    /// scaled it down.
    pub data: Bytes,
}
