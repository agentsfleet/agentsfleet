//! What sets one provider wire apart from the others: where a turn posts, how
//! the key rides, the body it sends, and how its stream reads.
//!
//! Everything else is shared, in [`Http`](crate::http::Http): the client, the
//! bounded retry and the Server-Sent Events framing. A wire is a [`Dialect`]
//! with a [`Decode`]r, so a fourth provider is one more pair, and nothing
//! else changes.

use std::fmt;

use eventsource_stream::Event;
use reqwest::RequestBuilder;

use crate::error::Result;
use crate::provider::{Chunk, Request};

/// One provider wire.
pub(crate) trait Dialect: Send + Sync + fmt::Debug + 'static {
    /// The wire's family, as a log line names it.
    const NAME: &'static str;
    /// Where a turn posts, after the provider's base URL.
    const PATH: &'static str;

    /// Reads one turn's stream.
    type Decoder: Decode + 'static;

    /// `builder` carrying `key` the way this wire takes it.
    fn authorize(&self, builder: RequestBuilder, key: &str) -> RequestBuilder;

    /// One turn's request body.
    ///
    /// # Errors
    /// The body would not encode.
    fn body(&self, request: &Request<'_>) -> Result<Vec<u8>>;

    /// A reader for one turn's stream.
    fn decoder(&self) -> Self::Decoder;
}

/// Reads one turn's stream, one event at a time.
pub(crate) trait Decode: Send {
    /// Reads `event`, handing each chunk it completes to `emit`, in order.
    ///
    /// # Errors
    /// The event is not the JSON its wire defines, or the provider ended the
    /// turn with an error of its own.
    fn event(&mut self, event: &Event, emit: &mut impl FnMut(Chunk)) -> Result<()>;

    /// Whether the turn has ended. A stream that closes before it has is a
    /// lost connection, not a short answer.
    fn ended(&self) -> bool;
}
