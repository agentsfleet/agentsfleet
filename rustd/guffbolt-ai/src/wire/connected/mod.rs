//! A route connected to its provider: the [`Model`] a loop streams turns from.
//!
//! Text and reasoning go out as they arrive; a tool call goes out whole once
//! its arguments end; when the stream is done, the turn's usage and its end
//! follow: the reasoning the provider wants back, and whether the turn stopped
//! at its output limit. A call the provider sent with no id is given one, so
//! the result it is answered with still matches.
//!
//! A turn the provider ends early, after its reply began but before anything
//! visible went out, is opened again under the same bound the transport's
//! retry keeps. Once text or a call has gone out live, a failure ends the
//! turn: a second pass would show it twice.
//!
//! A malformed call goes out with its raw argument text, so the tool refuses
//! it with a reason the model reads. Other calls and usage still arrive.

use std::fmt;
use std::sync::Arc;

use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use rig_core::ProviderError;

mod reopen;
use reopen::Pass;

use super::observe::Observer;
use super::request;
use super::retry::{Retried, Retry};
use super::route::Route;
use super::stream::{StreamTurn, TurnControl};
use super::timeouts::Timeouts;
use super::transport::{HttpTransport, Send};
use crate::{Chunk, Error, Model, Request, Result};

/// A [`Route`] with its key and transport: the [`Model`] its turns stream
/// from.
pub struct Connected {
    route: Route,
    model: Box<str>,
    key: Key,
    transport: Arc<dyn Send>,
    reply_bound: Option<usize>,
    retry: Retry,
    timeouts: Timeouts,
    observer: Option<Arc<dyn Observer>>,
    images: bool,
}

/// The provider's key, never printed.
struct Key(Box<str>);

impl fmt::Debug for Connected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connected")
            .field("api", &self.route.api())
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl Connected {
    /// `model` on `route`, authorized by `key`, its requests sent through
    /// `transport`: once each, with no reply bound, under the default
    /// [`Retry`] and [`Timeouts`].
    #[must_use]
    pub fn new(route: Route, model: &str, key: &str, transport: impl Send + 'static) -> Self {
        Self {
            route,
            model: model.into(),
            key: Key(key.into()),
            transport: Arc::new(transport),
            reply_bound: None,
            retry: Retry::DEFAULT,
            timeouts: Timeouts::DEFAULT,
            observer: None,
            images: true,
        }
    }

    /// The same model, its requests sent through `transport` instead.
    #[must_use]
    pub fn with_transport(mut self, transport: impl Send + 'static) -> Self {
        self.transport = Arc::new(transport);
        self
    }

    /// The same model, a reply past `bytes` refused rather than read on. It
    /// is never opened again: the next would be as long.
    #[must_use]
    pub const fn with_reply_bound(mut self, bytes: usize) -> Self {
        self.reply_bound = Some(bytes);
        self
    }

    /// The same model, sending and opening each turn under `retry`.
    #[must_use]
    pub const fn with_retry(mut self, retry: Retry) -> Self {
        self.retry = retry;
        self
    }

    /// The same model, reading each turn under `timeouts`.
    #[must_use]
    pub const fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    /// The same model, telling `observer` what each turn does.
    #[must_use]
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// The same model, its results' images offered only when `images`: a
    /// catalogue row says whether the model reads them, and the route's
    /// [`Api`](super::Api) whether its wire carries them.
    #[cfg(feature = "reqwest")]
    pub(crate) const fn with_images(mut self, images: bool) -> Self {
        self.images = images;
        self
    }

    /// The route the model is reached on.
    #[must_use]
    pub const fn route(&self) -> &Route {
        &self.route
    }

    /// Opens the turn `request` asks for: measured, confirmed by the
    /// provider's own count near the window, bounded to the output left, then
    /// sent.
    ///
    /// # Errors
    /// The conversation cannot be sent, the input leaves no room to answer,
    /// or rig could not start the reply.
    async fn open(&self, request: &Request<'_>) -> Result<StreamTurn> {
        let built = request::request(self.route.api(), request)?;
        let control = TurnControl {
            timeouts: self.timeouts,
            observer: self.observer.clone(),
            ..TurnControl::default()
        };
        let counting = HttpTransport::new(Arc::clone(&self.transport)).with_bound(self.reply_bound);
        let mut count = self
            .route
            .confirm_input(request.model, &self.key.0, &built, &counting, &control)
            .await
            .map_err(|failure| Error::provider(ProviderError::from(failure)))?;
        let built = self
            .route
            .bound_counted_output(request.model, built, &mut count)?;
        let retried = Retried::new(
            Arc::clone(&self.transport),
            self.retry,
            self.observer.clone(),
        );
        let streaming = HttpTransport::new(Arc::new(retried)).with_bound(self.reply_bound);
        let model = self.route.model(&self.model, &self.key.0, streaming);
        StreamTurn::open_counted(&self.route, &model, built, count, control)
    }

    /// `request` for the model this was connected to. A loop's request may
    /// name no model, as one over any [`Model`] knows no id, so the id the
    /// turn is counted, bounded and sent as is always the connected one.
    fn own<'a>(&'a self, request: Request<'a>) -> Request<'a> {
        Request {
            model: &self.model,
            ..request
        }
    }

    /// Tells the observer a turn is opened again after `failure`.
    fn reopened(&self, attempt: usize, failure: ProviderError) {
        if let Some(observer) = &self.observer {
            observer.reopened(attempt, &Error::provider(failure));
        }
    }
}

impl Model for Connected {
    type Error = Error;

    fn measure(&self, request: Request<'_>) -> Result<u64> {
        let request = self.own(request);
        let body = request::request(self.route.api(), &request)?;
        self.route.measure_input(request.model, &body)
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, Result<Chunk>> {
        let request = self.own(request);
        stream::unfold(Pass::Opening(1), move |pass| pass.next(self, request)).boxed()
    }

    fn accepts_images(&self) -> bool {
        self.images && self.route.api().carries_images()
    }
}

#[cfg(test)]
mod tests;
