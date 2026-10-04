//! The daemon's per-host request rules, evaluated as the daemon compiled them.
//!
//! A host with rules is reached only by a request one rule admits: the
//! method, the path exactly or by prefix, and every JSON field the rule locks.
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
    origin
        .requests
        .iter()
        .any(|rule| rule_admits(rule, method, path, fields.as_ref()))
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

fn rule_admits(
    rule: &HttpRequestRule<'_>,
    method: &Method,
    path: &str,
    fields: Option<&Fields>,
) -> bool {
    *method == method_of(rule.method)
        && path_admits(rule.path_match, &rule.path, path)
        && rule
            .json_fields
            .iter()
            .all(|locked| fields.is_some_and(|fields| field_admits(locked, fields)))
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
