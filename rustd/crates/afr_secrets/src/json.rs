//! One walk over a JSON value's keys and strings.
//!
//! Every rule that rewrites text inside a call's arguments (the secret scrub,
//! the leaf bound a trace row keeps) is a function handed to [`rewrite`], so
//! the recursion over arrays and objects is written once.

use serde_json::Value;

/// Rewrites every key and every string inside `value`, in place, through
/// `rule`: `Some` replaces the text, `None` keeps it. Numbers, booleans and
/// nulls are left alone.
pub fn rewrite(value: &mut Value, rule: &impl Fn(&str) -> Option<String>) {
    match value {
        Value::String(text) => {
            if let Some(rewritten) = rule(text) {
                *text = rewritten;
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| rewrite(item, rule)),
        Value::Object(fields) => {
            *fields = std::mem::take(fields)
                .into_iter()
                .map(|(key, mut field)| {
                    rewrite(&mut field, rule);
                    (rule(&key).unwrap_or(key), field)
                })
                .collect();
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
#[path = "json/tests.rs"]
mod tests;
