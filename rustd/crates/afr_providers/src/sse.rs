//! One turn's response, read as Server-Sent Events into chunks.
//!
//! `eventsource-stream` frames the bytes; the wire's [`Decode`]r reads each
//! event. A stream that closes before the decoder saw its turn end is a lost
//! connection, so a cut-off answer is never reported as a short one.

use std::collections::VecDeque;
use std::io;

use eventsource_stream::Eventsource as _;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use reqwest::Response;

use crate::dialect::Decode;
use crate::error::{Error, Result};
use crate::provider::Chunk;

/// The chunks `response` streams, read by `decoder`.
pub(crate) fn chunks<D: Decode + 'static>(
    response: Response,
    decoder: D,
) -> BoxStream<'static, Result<Chunk>> {
    let events = response.bytes_stream().eventsource().boxed();
    let pump = Pump {
        events,
        decoder,
        ready: VecDeque::new(),
        closed: false,
    };
    stream::unfold(pump, Pump::next).boxed()
}

/// The framed events still to read.
type Events = BoxStream<
    'static,
    std::result::Result<
        eventsource_stream::Event,
        eventsource_stream::EventStreamError<reqwest::Error>,
    >,
>;

/// The stream's state between chunks.
struct Pump<D> {
    events: Events,
    decoder: D,
    ready: VecDeque<Chunk>,
    closed: bool,
}

impl<D: Decode> Pump<D> {
    /// The next chunk, or the failure that ends the turn; `None` once it has
    /// ended.
    async fn next(mut self) -> Option<(Result<Chunk>, Self)> {
        loop {
            if let Some(chunk) = self.ready.pop_front() {
                return Some((Ok(chunk), self));
            }
            if self.closed {
                return None;
            }
            let failure = match self.events.next().await {
                None if self.decoder.ended() => {
                    self.closed = true;
                    continue;
                }
                None => Error::lost(io::Error::from(io::ErrorKind::UnexpectedEof)),
                Some(Err(framing)) => Error::lost(framing),
                Some(Ok(event)) => {
                    let ready = &mut self.ready;
                    match self
                        .decoder
                        .event(&event, &mut |chunk| ready.push_back(chunk))
                    {
                        Ok(()) => continue,
                        Err(failure) => failure,
                    }
                }
            };
            self.closed = true;
            return Some((Err(failure), self));
        }
    }
}
