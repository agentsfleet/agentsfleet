//! One run's model turns: `guffbolt_ai`'s connected model, its failures
//! named as a run's report names them.
//!
//! The wire streams the turn, retries its send and opens it again while
//! nothing was shown or spent (`guffbolt_ai::wire::Connected`). What is
//! agentsfleet's is here: the code and class a failure is reported under, an
//! endpoint the egress guard refused named as the policy's error, and the
//! run's log lines and measurements ([`observer`]).

use std::fmt;

use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use guffbolt::{Chunk, Model, Request};
use guffbolt_ai::wire::Connected;

use crate::error::{Error, Result, raise};

pub(crate) mod observer;

/// One run's provider: the connected model and whose run it serves.
pub(crate) struct Turns {
    connected: Connected,
    run_id: Box<str>,
    /// The provider as the policy spells it, named when its endpoint is refused.
    provider: Box<str>,
}

impl fmt::Debug for Turns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Turns")
            .field("wire", &self.connected.route().api())
            .field("run_id", &self.run_id)
            .finish_non_exhaustive()
    }
}

impl Turns {
    /// Run `run_id`'s provider, `provider` as the policy spells it.
    pub(crate) fn new(connected: Connected, run_id: &str, provider: &str) -> Self {
        Self {
            connected,
            run_id: run_id.into(),
            provider: provider.into(),
        }
    }

    /// `failure` as the kind a report names. An endpoint the client's guard
    /// refused is the policy's error, named as such, never a lost connection.
    fn failed(&self, failure: guffbolt_ai::Error) -> Error {
        match failure.into_provider_failure() {
            Ok(failure) if raise::blocked(&failure) => raise::blocked_endpoint(&self.provider),
            Ok(failure) => raise::provider(failure),
            Err(failure) => raise::wire(failure),
        }
    }
}

impl Model for Turns {
    type Error = Error;

    fn measure(&self, request: Request<'_>) -> Result<u64> {
        self.connected
            .measure(request)
            .map_err(|failure| self.failed(failure))
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>> {
        self.connected
            .stream(request)
            .map(|chunk| chunk.map_err(|failure| self.failed(failure)))
            .boxed()
    }

    fn accepts_images(&self) -> bool {
        self.connected.accepts_images()
    }
}

#[cfg(test)]
mod tests;
