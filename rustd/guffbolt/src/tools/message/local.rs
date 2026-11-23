//! The channel of a standalone harness: each line handed to a callback.

use std::fmt;

use super::{Channel, ChannelFuture, Posted};
use crate::transcript::CallRef;

/// What a line is handed to.
type Hear = dyn Fn(&str) + Send + Sync;

/// Hands each line to the embedder's callback, such as one that prints it.
pub struct LocalChannel {
    hear: Box<Hear>,
}

impl LocalChannel {
    /// A channel handing each line to `hear`.
    #[must_use]
    pub fn new(hear: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            hear: Box::new(hear),
        }
    }
}

impl fmt::Debug for LocalChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalChannel").finish_non_exhaustive()
    }
}

impl Channel for LocalChannel {
    fn post<'a>(&'a self, _call: &'a CallRef, text: &'a str) -> ChannelFuture<'a> {
        (self.hear)(text);
        Box::pin(std::future::ready(Ok(Posted::Delivered)))
    }
}
