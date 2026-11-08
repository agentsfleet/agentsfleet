use std::{fmt, pin::Pin, sync::Arc};

use futures_util::{Stream, StreamExt};

use crate::{Result, Subject, error::ErrorKind};

/// One shared payload; adapters preserve its allocation across subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Typed logical destination, independent of the broker's channel spelling.
    pub subject: Subject,
    /// UTF-8 JSON or a typed identifier, shared without copying its bytes.
    pub payload: Arc<String>,
}

/// A signal, a bounded-reader overrun or an unknown amount of lost traffic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    /// One message shared by all consumers on the subject.
    Message(Arc<Message>),
    /// This reader lost the given number of locally buffered messages.
    Lagged(u64),
    /// The native subscription was lost and restored; the missing count is unknown.
    Gap,
}

/// Owning stream; dropping it releases the adapter's native subscription.
pub struct Subscription {
    stream: Pin<Box<dyn Stream<Item = Result<Received>> + Send + 'static>>,
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription").finish_non_exhaustive()
    }
}

impl Subscription {
    /// Keep an adapter's stream and its drop behavior behind one boxed boundary.
    #[must_use]
    pub fn new(stream: impl Stream<Item = Result<Received>> + Send + 'static) -> Self {
        Self {
            stream: Box::pin(stream),
        }
    }

    /// Wait for the next signal or loss notification.
    ///
    /// # Errors
    /// Returns a closed subscription or the adapter's original failure.
    pub async fn recv(&mut self) -> Result<Received> {
        self.stream.next().await.ok_or(ErrorKind::Closed)?
    }
}
