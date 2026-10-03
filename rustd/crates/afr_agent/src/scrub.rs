//! The scrub: every known secret value becomes `«secret:NAME»`, and every NUL
//! becomes U+FFFD, before text reaches a frame, the trace, a record, the model
//! or the report.
//!
//! The values are the provider key and every string field of `secrets_map`
//! except `host`, which names where a credential goes rather than the
//! credential. A NUL is masked in the same pass because no stored trace or
//! record may hold one. A stream is scrubbed across its chunk boundaries: the
//! tail that could still be the start of a secret is held back until the next
//! chunk settles it, the shape `src/runner/engine/stream_redactor.zig` carries.

use std::borrow::Cow;
use std::ops::Deref;

use afd_wire::policy::ExecutionPolicy;
use aho_corasick::{AhoCorasick, MatchKind};
use serde_json::Value;

use crate::error::Result;
use crate::json;

/// The name the provider key is masked under.
const API_KEY_NAME: &str = "llm.api_key";
/// The credential field naming a credential's host, which is no secret.
const FIELD_HOST: &str = "host";
/// The character no stored trace or record may hold.
const NUL: &str = "\0";
/// What a NUL becomes.
const NUL_STAND_IN: &str = "\u{fffd}";

/// A value the scrub has passed: no secret value and no NUL is left in it.
///
/// Only [`Scrub`] builds one, so the trace, a record or a frame that takes a
/// `Clean` cannot be handed text the scrub never saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Clean<T>(T);

impl<T> Clean<T> {
    /// The scrubbed value.
    pub(crate) fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for Clean<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

/// The secret values of one run, and what each becomes.
#[derive(Debug)]
pub(crate) struct Scrub {
    matcher: AhoCorasick,
    /// The secret values alone, without the NUL, for the held-tail check.
    secrets: Vec<String>,
    /// One replacement per matcher pattern: each secret's mask, then the NUL's.
    replacements: Vec<String>,
}

impl Scrub {
    /// The scrub for a run under `policy`.
    ///
    /// # Errors
    /// The matcher could not be built over the secret values. The run fails
    /// rather than go on with nothing masked.
    pub(crate) fn new(policy: &ExecutionPolicy<'_>) -> Result<Self> {
        let mut secrets = vec![(API_KEY_NAME.to_owned(), policy.api_key.as_ref())];
        let credentials = policy.secrets_map.as_ref().and_then(Value::as_object);
        for (name, credential) in credentials.into_iter().flatten() {
            let fields = credential.as_object().into_iter().flatten();
            for (field, value) in fields.filter(|(field, _)| field.as_str() != FIELD_HOST) {
                if let Some(value) = value.as_str() {
                    secrets.push((format!("{name}.{field}"), value));
                }
            }
        }
        Self::of(secrets)
    }

    /// A scrub masking each `(name, value)`; an empty value masks nothing.
    fn of<'v>(named: impl IntoIterator<Item = (String, &'v str)>) -> Result<Self> {
        let (mut replacements, secrets): (Vec<String>, Vec<String>) = named
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| (format!("«secret:{name}»"), value.to_owned()))
            .unzip();
        replacements.push(NUL_STAND_IN.to_owned());
        let patterns = secrets.iter().map(String::as_str).chain([NUL]);
        // Leftmost-longest, so a secret that contains another is masked whole.
        let matcher = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(patterns)?;
        Ok(Self {
            matcher,
            secrets,
            replacements,
        })
    }

    /// `text` with every secret value and NUL masked, borrowed when there was
    /// nothing to mask.
    pub(crate) fn text<'t>(&self, text: &'t str) -> Cow<'t, str> {
        if self.matcher.is_match(text) {
            Cow::Owned(self.matcher.replace_all(text, &self.replacements))
        } else {
            Cow::Borrowed(text)
        }
    }

    /// `text`, masked; the same buffer when there was nothing to mask.
    pub(crate) fn clean(&self, text: String) -> Clean<String> {
        Clean(self.masked(&text).unwrap_or(text))
    }

    /// `value` with every string, and every key, masked.
    pub(crate) fn clean_json(&self, mut value: Value) -> Clean<Value> {
        json::rewrite(&mut value, &|text| self.masked(text));
        Clean(value)
    }

    /// The masked text, when masking changed it.
    fn masked(&self, text: &str) -> Option<String> {
        match self.text(text) {
            Cow::Owned(masked) => Some(masked),
            Cow::Borrowed(_) => None,
        }
    }

    /// How many trailing bytes of `text` could still be the start of a secret
    /// a later chunk completes, rounded out to a character boundary.
    pub(crate) fn pending(&self, text: &str) -> usize {
        let held = self
            .secrets
            .iter()
            .map(|secret| partial_head(text, secret))
            .max()
            .unwrap_or(0);
        text.len() - text.floor_char_boundary(text.len() - held)
    }
}

/// The longest proper prefix of `secret` that `text` ends with; a one-byte
/// secret has none.
fn partial_head(text: &str, secret: &str) -> usize {
    let (text, secret) = (text.as_bytes(), secret.as_bytes());
    (1..secret.len())
        .rev()
        .find(|&len| secret.get(..len).is_some_and(|head| text.ends_with(head)))
        .unwrap_or(0)
}

/// One stream's held tail, so a secret split across chunks is never sent in
/// pieces.
#[derive(Debug, Default)]
pub(crate) struct Carry {
    held: String,
}

impl Carry {
    /// Appends `chunk` and returns what is safe to send now, masked. The held
    /// buffer itself becomes the answer when nothing in it needed masking.
    pub(crate) fn push(&mut self, scrub: &Scrub, chunk: &str) -> String {
        self.held.push_str(chunk);
        let held = std::mem::take(&mut self.held);
        let mut ready = scrub.masked(&held).unwrap_or(held);
        let keep = scrub.pending(&ready);
        self.held = ready.split_off(ready.len() - keep);
        ready
    }
}

#[cfg(test)]
#[path = "scrub/tests.rs"]
mod tests;
