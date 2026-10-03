//! The secret scrub: every known secret value becomes `«secret:NAME»` before it
//! reaches a frame, the trace, a record, the model or the report.
//!
//! The values are the provider key and every string field of `secrets_map`
//! except `host`, which names where a credential goes rather than the
//! credential. A stream is scrubbed across its chunk boundaries: the tail that
//! could still be the start of a secret is held back until the next chunk
//! settles it, the shape `src/runner/engine/stream_redactor.zig` carries.

use std::borrow::Cow;

use afd_wire::policy::ExecutionPolicy;
use aho_corasick::{AhoCorasick, MatchKind};

/// The name the provider key is masked under.
const API_KEY_NAME: &str = "llm.api_key";
/// The credential field naming a credential's host, which is no secret.
const FIELD_HOST: &str = "host";

/// The secret values of one run, and the mask each becomes.
#[derive(Debug, Default)]
pub(crate) struct Scrub {
    matcher: Option<AhoCorasick>,
    values: Vec<String>,
    masks: Vec<String>,
}

impl Scrub {
    /// The scrub for a run under `policy`.
    #[must_use]
    pub(crate) fn new(policy: &ExecutionPolicy<'_>) -> Self {
        let mut secrets = vec![(API_KEY_NAME.to_owned(), policy.api_key.as_ref())];
        let credentials = policy.secrets_map.as_ref().and_then(|map| map.as_object());
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
    fn of<'v>(secrets: impl IntoIterator<Item = (String, &'v str)>) -> Self {
        let (masks, values): (Vec<String>, Vec<String>) = secrets
            .into_iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(name, value)| (format!("«secret:{name}»"), value.to_owned()))
            .unzip();
        // Leftmost-longest, so a secret that contains another is masked whole.
        let matcher = AhoCorasick::builder()
            .match_kind(MatchKind::LeftmostLongest)
            .build(&values)
            .ok()
            .filter(|_| !values.is_empty());
        Self {
            matcher,
            values,
            masks,
        }
    }

    /// `text` with every secret value masked.
    #[must_use]
    pub(crate) fn text<'t>(&self, text: &'t str) -> Cow<'t, str> {
        match &self.matcher {
            Some(matcher) if matcher.is_match(text) => {
                Cow::Owned(matcher.replace_all(text, &self.masks))
            }
            Some(_) | None => Cow::Borrowed(text),
        }
    }

    /// Masks every string, and every key, inside `value`.
    pub(crate) fn json(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                if let Cow::Owned(masked) = self.text(text) {
                    *text = masked;
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|item| self.json(item)),
            serde_json::Value::Object(fields) => {
                let masked = std::mem::take(fields)
                    .into_iter()
                    .map(|(key, mut field)| {
                        self.json(&mut field);
                        (self.text(&key).into_owned(), field)
                    })
                    .collect();
                *fields = masked;
            }
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            }
        }
    }

    /// How many trailing bytes of `text` could still be the start of a secret
    /// a later chunk completes, rounded out to a character boundary.
    #[must_use]
    pub(crate) fn pending(&self, text: &str) -> usize {
        let held = self
            .values
            .iter()
            .map(|value| partial_head(text, value))
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
    /// Appends `chunk` and returns what is safe to send now, masked.
    pub(crate) fn push(&mut self, scrub: &Scrub, chunk: &str) -> String {
        self.held.push_str(chunk);
        let masked = scrub.text(&self.held).into_owned();
        let keep = scrub.pending(&masked);
        let (ready, rest) = masked.split_at(masked.len() - keep);
        let ready = ready.to_owned();
        rest.clone_into(&mut self.held);
        ready
    }
}

#[cfg(test)]
#[path = "scrub/tests.rs"]
mod tests;
