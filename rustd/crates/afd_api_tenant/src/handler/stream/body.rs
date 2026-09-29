//! The live tail's response body: Server-Sent Events written from the payload
//! every viewer shares.
//!
//! # Why not `axum::response::sse::Sse`
//!
//! `Sse` renders each `Event` into a buffer of its own, so a frame watched by
//! a thousand tabs is a thousand copies of its payload — the per-viewer cost
//! the hub's shared `Arc<Message>` exists to remove. This body writes an
//! activity frame as three chunks: its own `id:`/`event:`/`data:` prefix, the
//! payload's bytes borrowed from the shared message, and the blank line that
//! ends the event. The bytes on the wire are exactly what `Sse` wrote — the
//! same field order, and a line break inside the data continued as another
//! `data:` line the way `Sse` continues it —
//! `the_body_writes_what_axum_would` holds that.
//!
//! # The heartbeat
//!
//! A named event, sent after [`afd_sse::HEARTBEAT_INTERVAL`] with nothing else
//! written, and re-armed by every event — heartbeats included — which is the
//! `KeepAlive` behaviour this replaces.
//!
//! # Why the slot rides the body
//!
//! [`afd_sse::Slot`] is released by `Drop`, and the body owns it: a client
//! that goes away drops the body, which returns the slot.

use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::{Arc, LazyLock};

use afd_dragonfly::Message;
use afd_sse::{Frame, Slot};
use axum::body::{Body, Bytes};
use axum::http::header;
use axum::response::{IntoResponse as _, Response};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use tokio::time::Instant;

/// The media type an `EventSource` requires.
const EVENT_STREAM: &str = "text/event-stream";

/// What `Sse` sends, so no intermediary caches a live tail.
const NO_CACHE: &str = "no-cache";

/// What opens every data line, the first and each continuation.
const DATA_FIELD: &str = "data: ";

/// The blank line that ends an event.
const END_OF_EVENT: &str = "\n\n";

/// The heartbeat event, rendered once for the process.
static HEARTBEAT: LazyLock<Bytes> = LazyLock::new(|| {
    Bytes::from(format!(
        "event: {}\n{DATA_FIELD}{}{END_OF_EVENT}",
        afd_sse::HEARTBEAT_EVENT,
        afd_sse::HEARTBEAT_DATA
    ))
});

/// One response body, holding `slot` for as long as it is alive.
pub(super) fn serve(frames: BoxStream<'static, Frame>, slot: Slot) -> Response {
    let wire = Wire {
        frames,
        _slot: slot,
        queued: VecDeque::new(),
        quiet_until: Instant::now() + afd_sse::HEARTBEAT_INTERVAL,
    };
    let chunks = stream::unfold(wire, Wire::next_chunk);
    (
        [
            (header::CONTENT_TYPE, EVENT_STREAM),
            (header::CACHE_CONTROL, NO_CACHE),
        ],
        Body::from_stream(chunks),
    )
        .into_response()
}

/// One connection's writer.
struct Wire {
    frames: BoxStream<'static, Frame>,
    /// Held for its `Drop`, never read.
    _slot: Slot,
    /// The rest of an event already started on the wire.
    queued: VecDeque<Bytes>,
    /// When the heartbeat is due if nothing is written first.
    quiet_until: Instant,
}

impl Wire {
    /// The next chunk: the rest of the current event, the next frame's first
    /// piece, or a heartbeat. `None` when the frames end, and then no
    /// heartbeat follows.
    async fn next_chunk(mut self) -> Option<(Result<Bytes, Infallible>, Self)> {
        if let Some(chunk) = self.queued.pop_front() {
            return Some((Ok(chunk), self));
        }
        let quiet_until = self.quiet_until;
        let arrived = tokio::select! {
            // A frame that is ready wins over a heartbeat that is due, as it
            // does under `KeepAlive`.
            biased;
            frame = self.frames.next() => Some(frame?),
            () = tokio::time::sleep_until(quiet_until) => None,
        };
        self.quiet_until = Instant::now() + afd_sse::HEARTBEAT_INTERVAL;
        let Some(frame) = arrived else {
            return Some((Ok(HEARTBEAT.clone()), self));
        };
        self.queued.extend(chunks(&frame));
        let first = self.queued.pop_front()?;
        Some((Ok(first), self))
    }
}

/// One frame as the chunks it is written in.
///
/// The `id:` line is this CONNECTION's counter. A browser sends it back as
/// `Last-Event-ID` on reconnect and this daemon ignores it — honouring it would
/// promise a resumption pub/sub cannot deliver, because it keeps nothing to
/// resume from. The client recovers the gap through the events list.
///
/// Shared only when it can be written verbatim: a data line holding a line
/// break has to be continued as several `data:` lines, which means rewriting
/// it, and that rare frame is rendered whole.
fn chunks(frame: &Frame) -> Vec<Bytes> {
    let mut prefix = format!("id: {}\nevent: {}\n", frame.seq, frame.kind);
    let data = &frame.data;
    if data.is_empty() {
        // `Sse` writes no data line for empty data at all.
        prefix.push('\n');
        return vec![Bytes::from(prefix)];
    }
    prefix.push_str(DATA_FIELD);
    let shared = data.shared().filter(|_verbatim| {
        !has_line_break(data.head()) && !has_line_break(data.tail()) && !data.tail().is_empty()
    });
    let Some((message, skip)) = shared else {
        continue_lines(&mut prefix, &data.text());
        prefix.push_str(END_OF_EVENT);
        return vec![Bytes::from(prefix)];
    };
    prefix.push_str(data.head());
    vec![
        Bytes::from(prefix),
        borrowed(message, skip),
        Bytes::from_static(END_OF_EVENT.as_bytes()),
    ]
}

/// The published payload's bytes from `skip` on, owned by the shared message
/// rather than copied out of it.
fn borrowed(message: &Arc<Message>, skip: usize) -> Bytes {
    let whole = Bytes::from_owner(Published(Arc::clone(message)));
    whole.slice(skip.min(whole.len())..)
}

/// A shared message, lent to `Bytes` as its payload's bytes.
struct Published(Arc<Message>);

impl AsRef<[u8]> for Published {
    fn as_ref(&self) -> &[u8] {
        self.0.payload.as_bytes()
    }
}

/// Whether `text` would end a line on the wire.
fn has_line_break(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// Appends `text`, continuing each line break as a new `data:` line — every
/// `\n` and `\r` kept and followed by `data: `, which is how `Sse` writes it.
fn continue_lines(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(at) = rest.find(['\n', '\r']) {
        // A line break is one byte, so `at + 1` is a character boundary.
        let (line, after) = rest.split_at(at + 1);
        out.push_str(line);
        out.push_str(DATA_FIELD);
        rest = after;
    }
    out.push_str(rest);
}
