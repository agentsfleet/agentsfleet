//! A frame's `data:` line, sharing the published payload rather than copying it.
//!
//! The hub hands every reader of a channel the same `Arc<Message>`. A frame
//! that copied the payload out of it would put the per-viewer copy straight
//! back — a 64 KiB answer watched by a thousand tabs is 64 MiB of copies per
//! frame — so an activity frame keeps the `Arc` and the HTTP body writes the
//! payload's bytes from it. A wall frame's `fleet_id` tag is a short head
//! written in front, followed by the payload from its second byte on: the
//! splice is two pieces on the wire, never a new string.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use afd_dragonfly::Message;

/// What follows `data: `, as a head this frame owns and a tail it shares.
#[derive(Clone)]
pub struct Data {
    /// Written first: all of a control frame's JSON, a wall frame's
    /// `{"fleet_id":"…"` tag, or nothing.
    head: String,
    /// The published frame whose payload follows the head, from `skip` on.
    shared: Option<Arc<Message>>,
    /// Bytes of the shared payload the head replaces — the `{` a tag opens
    /// in its place.
    skip: usize,
}

impl Data {
    /// Text this frame built and owns outright.
    pub(super) const fn owned(text: String) -> Self {
        Self {
            head: text,
            shared: None,
            skip: 0,
        }
    }

    /// The published payload, whole.
    pub(super) const fn published(message: Arc<Message>) -> Self {
        Self {
            head: String::new(),
            shared: Some(message),
            skip: 0,
        }
    }

    /// `head`, then the published payload from `skip` on.
    pub(super) const fn spliced(head: String, message: Arc<Message>, skip: usize) -> Self {
        Self {
            head,
            shared: Some(message),
            skip,
        }
    }

    /// The part this frame owns.
    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    /// The shared payload this frame ends with, and where in it the frame's
    /// bytes start — what an HTTP body writes without copying.
    #[must_use]
    pub fn shared(&self) -> Option<(&Arc<Message>, usize)> {
        self.shared.as_ref().map(|message| (message, self.skip))
    }

    /// The shared part as text: the payload from `skip` on, or nothing.
    #[must_use]
    pub fn tail(&self) -> &str {
        self.shared
            .as_deref()
            .and_then(|message| message.payload.get(self.skip..))
            .unwrap_or_default()
    }

    /// The whole line's text. Borrowed unless the frame has both a head and
    /// a shared part, which is the one case with two pieces to join.
    #[must_use]
    pub fn text(&self) -> Cow<'_, str> {
        match (self.head.is_empty(), self.shared.is_some()) {
            (_, false) => Cow::Borrowed(&self.head),
            (true, true) => Cow::Borrowed(self.tail()),
            (false, true) => Cow::Owned(format!("{}{}", self.head, self.tail())),
        }
    }

    /// Whether there is nothing to write at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.head.is_empty() && self.tail().is_empty()
    }
}

impl fmt::Display for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.head)?;
        f.write_str(self.tail())
    }
}

/// Shows the text a client would read, not the sharing behind it.
impl fmt::Debug for Data {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.text(), f)
    }
}

/// Two lines are equal when a client would read the same text from them.
impl PartialEq for Data {
    fn eq(&self, other: &Self) -> bool {
        *self == *other.text()
    }
}

impl Eq for Data {}

impl PartialEq<str> for Data {
    fn eq(&self, other: &str) -> bool {
        other
            .strip_prefix(self.head.as_str())
            .is_some_and(|rest| rest == self.tail())
    }
}

impl PartialEq<&str> for Data {
    fn eq(&self, other: &&str) -> bool {
        *self == **other
    }
}

impl PartialEq<String> for Data {
    fn eq(&self, other: &String) -> bool {
        *self == *other.as_str()
    }
}
