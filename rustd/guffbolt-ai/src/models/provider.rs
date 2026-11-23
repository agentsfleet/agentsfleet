//! One provider: where its wire is, the variable its key is read from, and
//! the catalogue rows of the models it serves.

use serde::Deserialize;

use crate::wire::Api;
use crate::wire::{Connected, Route, transport::Reqwest};
use crate::{Error, Result, error::ErrorKind};

/// One model's catalogue row: the numbers its turns are bounded by.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    /// The id the provider names the model by.
    pub id: String,
    /// The model's context window, in tokens.
    pub context_tokens: u32,
    /// The most one turn may write, in tokens.
    pub max_output_tokens: u32,
    /// Whether the model reads an image a call's result carries.
    pub images: bool,
}

/// Where a provider's key is read from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Key {
    /// The environment variable holding it.
    pub(crate) variable: &'static str,
    /// Every request needs it, so a model is not connected without it. A
    /// local server takes requests with none.
    pub(crate) required: bool,
}

impl Key {
    /// The key `read` answers for the variable. An empty value is no key.
    ///
    /// # Errors
    /// A required key's variable is unset or empty; the error names the
    /// variable, never a value.
    pub(crate) fn read(self, read: impl FnOnce(&'static str) -> Option<String>) -> Result<String> {
        match read(self.variable).filter(|key| !key.is_empty()) {
            Some(key) => Ok(key),
            None if self.required => Err(ErrorKind::MissingKey {
                variable: self.variable,
            }
            .into()),
            None => Ok(String::new()),
        }
    }
}

/// A provider a [`Models`](super::Models) collection reaches: its id, wire,
/// base URL, key variable and catalogue.
#[derive(Debug, Clone)]
pub struct Provider {
    id: &'static str,
    api: Api,
    dialect: Option<&'static str>,
    base_url: String,
    key: Key,
    catalog: &'static str,
    added: Vec<Spec>,
}

impl Provider {
    /// Provider `id` on `api` in rig's `dialect`, at `base_url`, its key
    /// read from `variable` and needed when `required`, its rows the JSON
    /// `catalog`.
    #[cfg_attr(
        not(any(
            test,
            feature = "anthropic",
            feature = "openai",
            feature = "gemini",
            feature = "ollama",
            feature = "openrouter",
            feature = "fireworks",
            feature = "minimax",
            feature = "deepseek",
            feature = "groq",
            feature = "mistral",
            feature = "xai",
            feature = "together",
        )),
        expect(dead_code, reason = "only a provider factory builds one")
    )]
    pub(crate) fn new(
        id: &'static str,
        api: Api,
        dialect: Option<&'static str>,
        base_url: &'static str,
        (variable, required): (&'static str, bool),
        catalog: &'static str,
    ) -> Self {
        Self {
            id,
            api,
            dialect,
            base_url: base_url.to_owned(),
            key: Key { variable, required },
            catalog,
            added: Vec::new(),
        }
    }

    /// Provider `id` on chat completions in rig's `dialect`, at the dialect's
    /// base URL, its key read from the dialect's variable.
    #[cfg(any(
        feature = "openrouter",
        feature = "deepseek",
        feature = "groq",
        feature = "mistral",
        feature = "xai",
        feature = "together",
    ))]
    pub(crate) fn chat(
        id: &'static str,
        dialect: &'static rig_core::providers::openai::wire::Dialect,
        catalog: &'static str,
    ) -> Self {
        Self::new(
            id,
            Api::Chat,
            Some(dialect.name),
            dialect.base_url,
            (dialect.api_key_env, true),
            catalog,
        )
    }

    /// The same provider, its requests sent under `base_url` instead: a
    /// proxy, a regional host or a local fake.
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// The same provider serving `spec` too, in place of a catalogue row of
    /// the same id: a model released after this catalogue was written.
    #[must_use]
    pub fn with_spec(mut self, spec: Spec) -> Self {
        self.added.retain(|added| added.id != spec.id);
        self.added.push(spec);
        self
    }

    /// The id the provider is found by.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    /// The base URL its requests are sent under.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The variable the provider's key is read from.
    #[must_use]
    pub const fn key_variable(&self) -> &'static str {
        self.key.variable
    }

    /// Every row the provider serves: its catalogue's, then those added.
    ///
    /// # Errors
    /// The compiled-in catalogue is not the rows it should hold.
    pub fn specs(&self) -> Result<Vec<Spec>> {
        let mut rows: Vec<Spec> = serde_json::from_str(self.catalog).map_err(|source| {
            Error::from(ErrorKind::Catalog {
                provider: self.id.into(),
                source,
            })
        })?;
        rows.retain(|row| self.added.iter().all(|added| added.id != row.id));
        rows.extend(self.added.iter().cloned());
        Ok(rows)
    }

    /// The row of model `id`.
    pub(crate) fn spec(&self, id: &str) -> Result<Spec> {
        self.specs()?
            .into_iter()
            .find(|row| row.id == id)
            .ok_or_else(|| {
                Error::from(ErrorKind::UnknownModel {
                    provider: self.id.into(),
                    model: id.into(),
                })
            })
    }

    pub(crate) const fn key(&self) -> Key {
        self.key
    }

    /// The model `spec` names, authorized by `key`, on the default transport.
    pub(crate) fn connect(&self, spec: &Spec, key: &str) -> Result<Connected> {
        let route = Route::new(
            self.api,
            &self.base_url,
            self.dialect,
            spec.max_output_tokens,
            spec.context_tokens,
        )?;
        let transport = Reqwest::new().map_err(|source| ErrorKind::Client { source })?;
        Ok(Connected::new(route, &spec.id, key, transport).with_images(spec.images))
    }
}
