//! The structural reading of a published schema a client would apply.
//!
//! Not a JSON Schema engine: it reads exactly the keywords the Definition's
//! components use (`$ref`, `oneOf`, `type`, `enum`, `required`, `properties`,
//! `additionalProperties`, `items` and the length, count and range bounds),
//! and an unknown keyword is a test failure rather than a silent pass.

#![expect(
    clippy::panic,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use regex::Regex;
use serde_json::{Map, Value};

/// The prefix every component reference carries.
const COMPONENT_REF: &str = "#/components/schemas/";

/// Keywords that describe a value without constraining it.
const ANNOTATIONS: [&str; 3] = ["description", "format", "title"];

/// Every published component, by name.
pub(crate) struct Components(pub(crate) Map<String, Value>);

impl Components {
    /// Whether `document` satisfies the component named `name`.
    pub(crate) fn admits(&self, name: &str, document: &Value) -> bool {
        self.check(&self.component(name), document)
    }

    fn component(&self, name: &str) -> Value {
        self.0
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("`{name}` is not a published component"))
    }

    fn check(&self, schema: &Value, document: &Value) -> bool {
        let Value::Object(rules) = schema else {
            panic!("a schema is an object: {schema}");
        };
        rules
            .iter()
            .all(|(keyword, rule)| self.keyword(rules, keyword, rule, document))
    }

    fn keyword(
        &self,
        rules: &Map<String, Value>,
        keyword: &str,
        rule: &Value,
        document: &Value,
    ) -> bool {
        match keyword {
            "$ref" => self.check(&self.component(reference(rule)), document),
            "oneOf" => {
                items(rule)
                    .filter(|variant| self.check(variant, document))
                    .count()
                    == 1
            }
            "type" => types(rule).any(|kind| is_a(kind, document)),
            "enum" => items(rule).any(|allowed| allowed == document),
            "required" => document.as_object().is_none_or(|object| {
                items(rule).all(|key| key.as_str().is_some_and(|key| object.contains_key(key)))
            }),
            "properties" => self.properties(rule, document),
            "additionalProperties" => closed_to(rules, rule, document),
            "items" => document
                .as_array()
                .is_none_or(|entries| entries.iter().all(|entry| self.check(rule, entry))),
            "minItems" | "maxItems" => count(keyword, rule, document.as_array().map(Vec::len)),
            "minLength" | "maxLength" => count(
                keyword,
                rule,
                document.as_str().map(|text| text.chars().count()),
            ),
            "minimum" | "maximum" => range(keyword, rule, document.as_f64()),
            "pattern" => document.as_str().is_none_or(|text| matches(rule, text)),
            annotation if ANNOTATIONS.contains(&annotation) => true,
            unknown => {
                panic!("the checker does not read `{unknown}`; teach it before relying on it")
            }
        }
    }

    fn properties(&self, rule: &Value, document: &Value) -> bool {
        let (Some(declared), Some(object)) = (rule.as_object(), document.as_object()) else {
            return true;
        };
        object.iter().all(|(key, value)| {
            declared
                .get(key)
                .is_none_or(|property| self.check(property, value))
        })
    }
}

/// The component a `$ref` names.
fn reference(rule: &Value) -> &str {
    rule.as_str()
        .and_then(|target| target.strip_prefix(COMPONENT_REF))
        .unwrap_or_else(|| panic!("an unexpected reference: {rule}"))
}

/// The entries of an array-valued keyword.
fn items(rule: &Value) -> impl Iterator<Item = &Value> {
    rule.as_array().into_iter().flatten()
}

/// The type names a `type` keyword allows, whether one or a list.
fn types(rule: &Value) -> impl Iterator<Item = &str> {
    rule.as_str()
        .into_iter()
        .chain(items(rule).filter_map(Value::as_str))
}

/// Whether `document` is of the JSON type `kind`.
fn is_a(kind: &str, document: &Value) -> bool {
    match kind {
        "null" => document.is_null(),
        "boolean" => document.is_boolean(),
        "string" => document.is_string(),
        "array" => document.is_array(),
        "object" => document.is_object(),
        "number" => document.is_number(),
        "integer" => document.is_i64() || document.is_u64(),
        other => panic!("an unexpected type: {other}"),
    }
}

/// Whether an object lists only declared keys, when the schema closes it.
fn closed_to(rules: &Map<String, Value>, rule: &Value, document: &Value) -> bool {
    assert!(
        rule == &Value::Bool(false),
        "only a closed `additionalProperties` is published here: {rule}"
    );
    let declared = rules.get("properties").and_then(Value::as_object);
    document.as_object().is_none_or(|object| {
        object
            .keys()
            .all(|key| declared.is_some_and(|declared| declared.contains_key(key)))
    })
}

/// Whether a measured length or count sits on the right side of its bound.
fn count(keyword: &str, rule: &Value, measured: Option<usize>) -> bool {
    let bound = rule.as_u64().and_then(|bound| usize::try_from(bound).ok());
    match (measured, bound) {
        (None, _) => true,
        (Some(measured), Some(bound)) if keyword.starts_with("min") => measured >= bound,
        (Some(measured), Some(bound)) => measured <= bound,
        (Some(_), None) => panic!("`{keyword}` is not a count: {rule}"),
    }
}

/// Whether a number sits on the right side of its bound.
fn range(keyword: &str, rule: &Value, number: Option<f64>) -> bool {
    let bound = rule
        .as_f64()
        .unwrap_or_else(|| panic!("`{keyword}` is not a number: {rule}"));
    number.is_none_or(|number| {
        if keyword == "minimum" {
            number >= bound
        } else {
            number <= bound
        }
    })
}

/// Whether `text` matches the published pattern.
fn matches(rule: &Value, text: &str) -> bool {
    let pattern = rule
        .as_str()
        .unwrap_or_else(|| panic!("a pattern is a string: {rule}"));
    Regex::new(pattern)
        .unwrap_or_else(|_invalid| panic!("`{pattern}` is not a valid pattern"))
        .is_match(text)
}
