//! The credential placeholder grammar: `${secrets.NAME.FIELD}`.
//!
//! A fleet's request names a secret by placeholder and never holds its value;
//! the guard puts the value in place at send time. The grammar is matched by
//! one regular expression (Indy, Oct 03: "Use regex"), and any text that
//! opens a placeholder the grammar does not match is treated as a placeholder
//! all the same, so a near-miss cannot slip a half-substituted string past the
//! placement rules.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::{Captures, Regex};

/// What every placeholder opens with.
pub(crate) const OPENING: &str = "${secrets.";

/// One placeholder, `${secrets.NAME.FIELD}`.
const GRAMMAR: &str = r"\$\{secrets\.([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)\}";

/// A URL whose whole host is `${secrets.NAME.host}`: the scheme, the name, and
/// whatever follows the host.
const HOST_URL: &str = r"^https://\$\{secrets\.([A-Za-z_][A-Za-z0-9_]*)\.host\}([/?#:].*)?$";

#[expect(
    clippy::expect_used,
    reason = "a literal pattern, compiled by every test in this module"
)]
static PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(GRAMMAR).expect("the placeholder grammar is a valid pattern"));

#[expect(
    clippy::expect_used,
    reason = "a literal pattern; the host-URL tests compile it"
)]
static HOST_PLACEHOLDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(HOST_URL).expect("the host-URL grammar is a valid pattern"));

/// One placeholder: which credential, and which of its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SecretRef<'t> {
    /// The credential's name.
    pub(crate) name: &'t str,
    /// The field.
    pub(crate) field: &'t str,
}

impl<'t> SecretRef<'t> {
    fn of(captures: &Captures<'t>) -> Option<Self> {
        Some(Self {
            name: captures.get(1)?.as_str(),
            field: captures.get(2)?.as_str(),
        })
    }
}

/// Whether `text` opens a placeholder anywhere.
pub(crate) fn mentions(text: &str) -> bool {
    text.contains(OPENING)
}

/// Every placeholder in `text`, or `None` when `text` opens one the grammar
/// does not match.
pub(crate) fn parse(text: &str) -> Option<Vec<SecretRef<'_>>> {
    let found: Vec<SecretRef<'_>> = PLACEHOLDER
        .captures_iter(text)
        .filter_map(|captures| SecretRef::of(&captures))
        .collect();
    (found.len() == text.matches(OPENING).count()).then_some(found)
}

/// The credential a URL's whole host names, and what follows the host; `None`
/// when the URL's host is no placeholder.
pub(crate) fn host_url(url: &str) -> Option<(&str, &str)> {
    let captures = HOST_PLACEHOLDER.captures(url)?;
    let name = captures.get(1)?.as_str();
    let rest = captures.get(2).map_or("", |rest| rest.as_str());
    Some((name, rest))
}

/// `text` with each placeholder replaced by what `value` answers for it.
///
/// A placeholder `value` answers `None` for stays as written; the caller
/// resolves every placeholder before it asks, so none does.
pub(crate) fn substitute<'v>(
    text: &str,
    value: impl Fn(SecretRef<'_>) -> Option<&'v str>,
) -> Cow<'_, str> {
    PLACEHOLDER.replace_all(text, |captures: &Captures<'_>| {
        let whole = captures.get(0).map_or("", |whole| whole.as_str());
        SecretRef::of(captures)
            .and_then(&value)
            .unwrap_or(whole)
            .to_owned()
    })
}

#[cfg(test)]
#[path = "placeholder/tests.rs"]
mod tests;
