//! What a person or a parent sends a loop, media included.
//!
//! A message is text and media parts. For each media part the embedder's
//! [`MediaReader`] supplies the text the model reads in its place: a voice
//! note's transcript, a document's text, a video's transcript or an image's
//! description. Until an embedder reads a kind it installs [`Unread`], and the
//! part reaches the model as a note naming it, never dropped without a word.
//! The kinds follow Codex's `UserInput` (text, image, audio), with video and
//! documents beside them.

use std::fmt;
use std::sync::Arc;

/// What joins the parts of one message, as the model reads it.
pub const PART_JOIN: &str = "\n\n";

/// The kinds of media a message can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// A picture: a screenshot, a photo, a diagram.
    Image,
    /// Sound: a voice note, a recorded call.
    Audio,
    /// Moving pictures, with or without sound.
    Video,
    /// A file to read: a PDF, a spreadsheet, a text file.
    Document,
}

impl MediaKind {
    /// The kind's name, as a note to the model spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Document => "document",
        }
    }
}

/// One media part: its bytes, and what they are.
///
/// The bytes are shared, so a part handed to a reader and kept in a record
/// is one allocation, however large the recording.
#[derive(Clone, PartialEq, Eq)]
pub struct Media {
    /// What the part is.
    pub kind: MediaKind,
    /// Its media type, as the sender named it: `audio/ogg`, `application/pdf`.
    pub media_type: String,
    /// The name it was sent under, when it had one.
    pub name: Option<String>,
    /// Its bytes.
    pub bytes: Arc<[u8]>,
}

impl fmt::Debug for Media {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Media")
            .field("kind", &self.kind)
            .field("media_type", &self.media_type)
            .field("name", &self.name)
            .field("bytes", &self.bytes.len())
            .finish()
    }
}

/// One part of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Text, as it was written.
    Text(String),
    /// A media part.
    Media(Media),
}

/// Why a media part reached the model as a note rather than its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// This embedder reads no part of this kind.
    Unsupported(MediaKind),
    /// The reader tried and failed, for the reason given.
    Failed(String),
}

/// Reads a media part into the text a model reads in its place. A reader
/// writes `read` as an `async fn`; its future must be `Send`.
pub trait MediaReader: Send + Sync {
    /// The text the model reads for `media`, or why the part cannot be read:
    /// its kind is unsupported, or reading it failed.
    fn read(&self, media: &Media) -> impl Future<Output = Result<String, Unreadable>> + Send;
}

/// The reader of an embedder that reads no media yet: every part is named
/// to the model and its content withheld.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unread;

impl MediaReader for Unread {
    fn read(&self, media: &Media) -> impl Future<Output = Result<String, Unreadable>> + Send {
        std::future::ready(Err(Unreadable::Unsupported(media.kind)))
    }
}

/// The text a model reads for `parts`: text as written, each media part
/// through `reader`, joined with blank lines. An unreadable part becomes a
/// note naming it, so the model can say what it did not see.
pub async fn render<R: MediaReader>(parts: &[Part], reader: &R) -> String {
    let mut rendered = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            Part::Text(text) => rendered.push(text.clone()),
            Part::Media(media) => rendered.push(match reader.read(media).await {
                Ok(text) => text,
                Err(why) => note(media, &why),
            }),
        }
    }
    rendered.join(PART_JOIN)
}

/// The note an unreadable part becomes: its kind, its name, its size, and why.
fn note(media: &Media, why: &Unreadable) -> String {
    let kind = media.kind.as_str();
    let named = media
        .name
        .as_deref()
        .map_or_else(String::new, |name| format!(" \"{name}\""));
    let size = media.bytes.len();
    match why {
        Unreadable::Unsupported(_) => {
            format!("[{kind}{named}, {size} bytes: this run cannot read {kind} content]")
        }
        Unreadable::Failed(reason) => {
            format!("[{kind}{named}, {size} bytes: it could not be read: {reason}]")
        }
    }
}

#[cfg(test)]
mod tests;
