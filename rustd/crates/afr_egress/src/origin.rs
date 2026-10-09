//! The daemon's per-host request rules, evaluated as the daemon compiled them.
//!
//! A host with rules is reached only by a request one rule admits: the
//! method, the path exactly or by prefix, and every JSON field the rule locks.
//! A rule that lists its permitted fields is closed: it admits no other
//! top-level key and no query string, so an unnamed parameter cannot widen
//! what its locked fields bound.
//! The rules are never re-derived here (`afd_gate::policy::egress` compiles
//! them), so the runner and the daemon cannot disagree about what a rule
//! means, only about whether a request matches it, which this module decides.
//!
//! Before any rule is read the path must be one the rules can be trusted
//! with: on port 443, with no encoded dot, slash or backslash a server might
//! decode into a traversal the rule never saw. A body whose top level repeats
//! a key admits no locked field: serde keeps the last of two, an upstream may
//! read the first, and the rule would have checked a value never sent.

use afd_wire::policy::{
    HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
};
use std::fmt;

use reqwest::{Method, Url};
use serde::de::{self, Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::{Map, Value};

/// The one port a host with rules is reached on.
const HTTPS_PORT: u16 = 443;

/// Escapes a server may decode into `.`, `/` or `\` after the rule matched
/// the undecoded path.
const ENCODED_SEPARATORS: [&str; 3] = ["%2e", "%2f", "%5c"];

/// Path segments that walk the tree instead of naming a place in it.
const DOT_SEGMENTS: [&str; 2] = [".", ".."];

/// What a body a rule can read is.
const ONE_OBJECT: &str = "a JSON object whose keys are each written once";

/// The most of a refused key a refusal repeats back to the model.
const KEY_ECHO_MAX: usize = 64;

/// Why a closed rule refused a request its method and path match: a query.
const QUERY_REFUSED: &str = "it carries a query string, which this rule does not admit";

/// Why a closed rule at `method` and `url`'s path refused this request, said
/// so the model can correct it: the query string, or the first top-level key
/// the rule does not list. `None` when no closed rule covers that method and
/// path, or the body names no key to blame.
pub(crate) fn closed_refusal(
    origin: &HttpOriginPolicy<'_>,
    method: &Method,
    url: &Url,
    body: Option<&str>,
) -> Option<String> {
    // A path `admits` refuses before reading any rule is no rule's to explain:
    // blaming a key or the query would send the model to fix the wrong thing.
    if !trusted_path(url) {
        return None;
    }
    let path = url.path();
    let rule = origin.requests.iter().find(|rule| {
        *method == method_of(rule.method)
            && path_admits(rule.path_match, &rule.path, path)
            && closed(rule)
    })?;
    if url.query().is_some() {
        return Some(QUERY_REFUSED.to_owned());
    }
    let fields = body.and_then(|body| serde_json::from_str::<Fields>(body).ok())?;
    let key = fields.0.keys().find(|key| !named(rule, key))?;
    Some(key_refused(key))
}

/// The sentence naming the first key a closed rule does not list.
///
/// The key is the model's own input, so it is cut to [`KEY_ECHO_MAX`]
/// characters and quoted with its escapes: a newline, a quote or a backtick in
/// it cannot reshape the refusal the model reads.
fn key_refused(key: &str) -> String {
    let shown: String = key.chars().take(KEY_ECHO_MAX).collect();
    format!("it sends {shown:?}, which this rule does not list")
}

/// Whether `origin`'s rules admit `method` at `url` carrying `body`.
pub(crate) fn admits(
    origin: &HttpOriginPolicy<'_>,
    method: &Method,
    url: &Url,
    body: Option<&str>,
) -> bool {
    if !trusted_path(url) {
        return false;
    }
    let path = url.path();
    let fields = body.and_then(|body| serde_json::from_str::<Fields>(body).ok());
    let sent = Sent {
        query: url.query().is_some(),
        body: body.is_some(),
        fields: fields.as_ref(),
    };
    origin
        .requests
        .iter()
        .any(|rule| rule_admits(rule, method, path, &sent))
}

/// What a request carries beyond its method and path.
struct Sent<'f> {
    /// Whether the URL has a query string, empty or not.
    query: bool,
    /// Whether a body was sent at all.
    body: bool,
    /// The body's top-level fields, when it is an object written once.
    fields: Option<&'f Fields>,
}

/// Whether `url` is on port 443 and its path holds no escape or dot segment.
fn trusted_path(url: &Url) -> bool {
    let path = url.path();
    let lowered = path.to_ascii_lowercase();
    url.port_or_known_default() == Some(HTTPS_PORT)
        && !path.contains('\\')
        && !ENCODED_SEPARATORS
            .iter()
            .any(|escape| lowered.contains(escape))
        && !path
            .split('/')
            .any(|segment| DOT_SEGMENTS.contains(&segment))
}

fn rule_admits(rule: &HttpRequestRule<'_>, method: &Method, path: &str, sent: &Sent<'_>) -> bool {
    *method == method_of(rule.method)
        && path_admits(rule.path_match, &rule.path, path)
        && rule.json_fields.iter().all(|locked| {
            sent.fields
                .is_some_and(|fields| field_admits(locked, fields))
        })
        && (!closed(rule) || sends_only_named(rule, sent))
}

/// Whether `rule` lists its whole key set, which closes it to every other.
///
/// A rule without `permitted_fields` is open: a read today's daemon writes, or
/// any rule from a daemon older than the field. Its locked fields are checked
/// and nothing else (`afd_wire::policy::HttpRequestRule`).
const fn closed(rule: &HttpRequestRule<'_>) -> bool {
    rule.permitted_fields.is_some()
}

/// Whether `sent` carries no query and no top-level key `rule` does not name.
///
/// A body that is not an object written once has keys nobody can list, so it
/// is refused here; no body at all carries none.
fn sends_only_named(rule: &HttpRequestRule<'_>, sent: &Sent<'_>) -> bool {
    let keys_named = match (sent.body, sent.fields) {
        (false, _) => true,
        (true, Some(fields)) => fields.0.keys().all(|key| named(rule, key)),
        (true, None) => false,
    };
    !sent.query && keys_named
}

/// Whether `key` is one of the fields `rule` locks or permits.
fn named(rule: &HttpRequestRule<'_>, key: &str) -> bool {
    rule.json_fields.iter().any(|locked| locked.name == key)
        || rule
            .permitted_fields
            .iter()
            .flatten()
            .any(|permitted| permitted == key)
}

const fn method_of(method: HttpMethod) -> Method {
    match method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Head => Method::HEAD,
        HttpMethod::Post => Method::POST,
    }
}

fn path_admits(matching: HttpPathMatch, rule: &str, path: &str) -> bool {
    match matching {
        HttpPathMatch::Exact => path == rule,
        HttpPathMatch::Prefix => path.starts_with(rule),
    }
}

/// Whether the body's top-level field holds exactly the value `locked` names.
/// A rule locking neither a string nor a boolean admits nothing.
fn field_admits(locked: &HttpJsonFieldRule<'_>, fields: &Fields) -> bool {
    let Some(value) = fields.0.get(locked.name.as_ref()) else {
        return false;
    };
    match (&locked.string_value, locked.boolean_value) {
        (Some(expected), None) => value.as_str() == Some(expected.as_ref()),
        (None, Some(expected)) => value.as_bool() == Some(expected),
        _unlocked => false,
    }
}

/// A body's top-level fields, each key written once.
struct Fields(Map<String, Value>);

impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(FieldsVisitor)
    }
}

/// Reads a body's top level, refusing the second of two equal keys.
struct FieldsVisitor;

impl<'de> Visitor<'de> for FieldsVisitor {
    type Value = Fields;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(ONE_OBJECT)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Fields, A::Error> {
        let mut fields = Map::new();
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            if fields.insert(key, value).is_some() {
                return Err(de::Error::custom(ONE_OBJECT));
            }
        }
        Ok(Fields(fields))
    }
}

#[cfg(test)]
#[path = "origin/tests.rs"]
mod tests;
