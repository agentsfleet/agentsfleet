//! The providers a program can reach, and the models each one serves.
//!
//! ```text
//!  Models::new().with(providers::minimax())
//!    get("minimax", id)      ──► the catalogue row: window, output, images
//!    connect("minimax", id)  ──► the row, the key from MINIMAX_API_KEY, a Route
//!                                └─► Connected, on the default transport
//! ```
//!
//! A provider is a factory's value: its id, its wire, its base URL, the
//! variable its key is read from, and its catalogue rows. A model no row
//! names is reached with [`Route::new`](crate::wire::Route::new) and
//! [`Connected::new`](crate::wire::Connected::new) instead.

mod provider;

pub use self::provider::{Provider, Spec};

use crate::wire::Connected;
use crate::{Error, Result, error::ErrorKind};

/// A collection of [`Provider`]s, each found by its id.
#[derive(Debug, Default)]
pub struct Models {
    providers: Vec<Provider>,
}

impl Models {
    /// No provider at all.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Every provider this build compiled in, one per enabled provider
    /// feature.
    #[must_use]
    pub fn builtin() -> Self {
        crate::providers::compiled()
            .into_iter()
            .fold(Self::new(), Self::with)
    }

    /// The collection with `provider` added, in place of any provider of the
    /// same id.
    #[must_use]
    pub fn with(mut self, provider: Provider) -> Self {
        self.providers.retain(|held| held.id() != provider.id());
        self.providers.push(provider);
        self
    }

    /// The row `provider` holds for model `id`.
    ///
    /// # Errors
    /// No provider of that id was added, its catalogue holds no such model,
    /// or the catalogue cannot be read.
    pub fn get(&self, provider: &str, id: &str) -> Result<Spec> {
        self.provider(provider)?.spec(id)
    }

    /// Model `id` of `provider`, its key read from the provider's variable,
    /// on the default transport.
    ///
    /// # Errors
    /// As [`get`](Self::get); or the provider's key variable is unset or
    /// empty, and the error names the variable and never a value; or the
    /// row or the transport cannot be built.
    #[expect(
        clippy::disallowed_methods,
        reason = "guffbolt-ai links no agentsfleet crate, so a provider's key is read from the process environment here, its one reader"
    )]
    pub fn connect(&self, provider: &str, id: &str) -> Result<Connected> {
        self.connect_reading(provider, id, |variable| std::env::var(variable).ok())
    }

    /// Model `id` of `provider`, authorized by `key`, on the default
    /// transport. [`Connected::with_transport`] replaces it.
    ///
    /// # Errors
    /// As [`get`](Self::get); or the row or the transport cannot be built.
    pub fn connect_with_key(&self, provider: &str, id: &str, key: &str) -> Result<Connected> {
        let provider = self.provider(provider)?;
        provider.connect(&provider.spec(id)?, key)
    }

    /// [`connect`](Self::connect), its key looked up through `read`. The
    /// model is found before the key is read, so an unknown one is refused
    /// whatever the environment holds.
    fn connect_reading(
        &self,
        provider: &str,
        id: &str,
        read: impl FnOnce(&'static str) -> Option<String>,
    ) -> Result<Connected> {
        let provider = self.provider(provider)?;
        let spec = provider.spec(id)?;
        let key = provider.key().read(read)?;
        provider.connect(&spec, &key)
    }

    fn provider(&self, id: &str) -> Result<&Provider> {
        self.providers
            .iter()
            .find(|provider| provider.id() == id)
            .ok_or_else(|| {
                Error::from(ErrorKind::Unregistered {
                    provider: id.into(),
                })
            })
    }
}

#[cfg(test)]
mod tests;
