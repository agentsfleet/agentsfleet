//! One Server-Sent Events frame, and the three shapes this daemon emits.
//!
//! A frame is `id:` (the per-connection sequence), `event:` (the payload's
//! `kind`), and `data:` (JSON). Rendering those three lines is the HTTP layer's
//! job — this module decides what goes IN them, so the decision is testable
//! without a socket.
//!
//! # Sequence numbers are per connection, and start at zero
//!
//! They are not durable positions in the log. A client that reconnects gets a
//! stream numbered from zero again and recovers the gap through the events
//! list, which is why `Last-Event-ID` is ignored rather than honoured: honouring
//! it would promise a resumption this daemon cannot deliver, because pub/sub
//! keeps nothing to resume FROM.
//!
//! # A frame the multiplex cannot route is dropped, never guessed
//!
//! Delivering an activity frame to the wrong fleet's tile is worse than losing
//! it: the tile shows another fleet's work as its own, and nothing in the
//! client can tell. The durable row is still there to be paged.

use std::borrow::Cow;

use crate::error::{Error, Result};

/// The `event:` name a payload with no readable `kind` is given.
pub const DEFAULT_KIND: &str = "message";

/// The `event:` name of the frame announcing a multiplex's fleet set.
pub const KIND_HELLO: &str = "hello";

/// The `event:` name of the frame announcing frames the server dropped.
pub const KIND_CATCHING_UP: &str = "catching_up";

/// The synthetic frames' sequence number.
///
/// Zero, and deliberately not a number from the connection's counter: `hello`
/// and `catching_up` are the server talking about the stream rather than
/// activity on it, and burning a sequence number on one would leave a gap in
/// the ids a client uses to tell a dropped frame from a control frame.
const SYNTHETIC_SEQ: u64 = 0;

/// The anchor `kind` is read from — the payload's LEADING field.
///
/// Anchored rather than searched so an embedded `"kind":"` inside a string
/// value cannot decide the dispatch. A publisher whose shape drifts loses the
/// name and gets [`DEFAULT_KIND`], which is the right failure: the frame still
/// arrives.
///
/// Spelled as a literal because a `const` cannot be formatted at compile time
/// without a macro crate this workspace does not carry for one string. What
/// keeps it from drifting away from [`KIND_KEY`] is
/// `tests::should_anchor_on_the_same_key_the_control_frames_write`, which
/// rebuilds it and compares — the tie is asserted rather than assumed.
const KIND_ANCHOR: &str = "{\"kind\":\"";

/// The key the multiplex splices into every frame it forwards.
const TAG_KEY: &str = "fleet_id";

/// The key every frame names its own shape under.
///
/// One spelling for both control frames and for the anchor the activity frames
/// are read through (RULE UFS). Two literals would be two things that can
/// drift, and a `hello` whose key drifted would reach a client as a frame with
/// no readable shape — arriving, and meaning nothing.
const KIND_KEY: &str = "kind";

/// One frame, decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The `id:` line — this connection's counter, or zero on a control frame.
    pub seq: u64,
    /// The `event:` line.
    pub kind: Cow<'static, str>,
    /// The `data:` line, already JSON.
    pub data: String,
}

impl Frame {
    /// One publisher's payload, forwarded unrewritten.
    #[must_use]
    pub fn activity(seq: u64, payload: String) -> Self {
        let kind = kind_of(&payload).unwrap_or(DEFAULT_KIND).to_owned();
        Self {
            seq,
            kind: Cow::Owned(kind),
            data: payload,
        }
    }

    /// The same payload with the originating fleet spliced in as its first key.
    ///
    /// The `kind` is read from the ORIGINAL payload, before the splice: the
    /// anchor is the leading field, and a tag written first would displace it.
    ///
    /// # Errors
    /// [`Error::Untaggable`] when the payload is not a JSON object, which is
    /// the only shape a key can be spliced into. The caller drops the frame
    /// rather than emit one a client could route to the wrong tile.
    pub fn tagged(seq: u64, fleet_id: &str, payload: &str) -> Result<Self> {
        let kind = kind_of(payload).unwrap_or(DEFAULT_KIND).to_owned();
        Self::splice(fleet_id, payload).map(|data| Self {
            seq,
            kind: Cow::Owned(kind),
            data,
        })
    }

    /// The control frame announcing which fleets this connection carries.
    #[must_use]
    pub fn hello(fleet_ids: &[String]) -> Self {
        let data = serde_json::json!({ KIND_KEY: KIND_HELLO, "fleet_ids": fleet_ids });
        Self {
            seq: SYNTHETIC_SEQ,
            kind: Cow::Borrowed(KIND_HELLO),
            data: data.to_string(),
        }
    }

    /// The control frame announcing frames the SERVER dropped, never the client.
    ///
    /// `dropped` is the count since the last such frame, so a client can add
    /// them up rather than having to diff two totals.
    #[must_use]
    pub fn catching_up(dropped: u64) -> Self {
        let data = serde_json::json!({ KIND_KEY: KIND_CATCHING_UP, "dropped": dropped });
        Self {
            seq: SYNTHETIC_SEQ,
            kind: Cow::Borrowed(KIND_CATCHING_UP),
            data: data.to_string(),
        }
    }

    /// `{"fleet_id":"…", …the publisher's fields}`, byte for byte.
    ///
    /// The payload is spliced rather than parsed and re-emitted: it is already
    /// valid JSON that this process did not author, and re-serializing it would
    /// reorder keys, reformat numbers, and make every publisher shape change
    /// this crate's problem.
    fn splice(fleet_id: &str, payload: &str) -> Result<String> {
        let body = payload
            .strip_prefix('{')
            .and_then(|open| open.strip_suffix('}'))
            .ok_or(Error::Untaggable)?;
        let tag = serde_json::to_string(fleet_id).map_err(|_unencodable| Error::Untaggable)?;
        let mut spliced = format!("{{\"{TAG_KEY}\":{tag}");
        if !body.trim().is_empty() {
            spliced.push(',');
            spliced.push_str(body);
        }
        spliced.push('}');
        Ok(spliced)
    }
}

/// The `kind` a payload names, when it names one as its leading field.
///
/// A kind carrying a control character is refused, and that is a SAFETY check
/// rather than tidiness: this value becomes the `event:` line, and
/// `axum::response::sse::Event::event` PANICS on a newline or carriage return
/// (`axum` `response/sse.rs`: "Panics if `event` contains any newlines or
/// carriage returns"). The kind is read by slicing the publisher's bytes rather
/// than by parsing them, so nothing upstream has ruled one out.
///
/// No current publisher can produce one — `afd_fleet::lease::activity` builds
/// every frame through `serde`, which escapes a newline to the two characters
/// `\n` — so this is depth, not a live defect. It is still the right shape:
/// the module's contract is that a publisher whose payload drifts loses its
/// name and gets [`DEFAULT_KIND`] while the frame still arrives, and a panic in
/// a daemon is not that contract. It would take down the one connection the
/// frame was bound for, and leave the client to guess why its stream ended.
#[must_use]
pub fn kind_of(payload: &str) -> Option<&str> {
    let rest = payload.strip_prefix(KIND_ANCHOR)?;
    let close = rest.find('"')?;
    let kind = &rest[..close];
    let printable = !kind.is_empty() && !kind.chars().any(char::is_control);
    printable.then_some(kind)
}

#[cfg(test)]
mod tests;
