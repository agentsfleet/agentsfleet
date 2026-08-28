//! YAML frontmatter rendered as the JSON the stored schema reads.
//!
//! # A maintained parser decides the types, not a table we own
//!
//! This module used to carry a port of `yaml_frontmatter.zig`'s `writeScalar`
//! — a coercion table sitting on a bare tokeniser, resolving every scalar by
//! hand — on the reasoning that a YAML crate would type scalars WRONG for a
//! parity port. That reasoning was tested against the wrong crate. Probed
//! against `yaml_serde` (the YAML 1.2 core schema, maintained by the YAML
//! organisation) the two agree on every case the table existed to defend:
//! `01` stays `"01"`, `0123456` stays a string, `NO` does not become false,
//! `yes` and `on` stay strings, and `NaN` stays text.
//!
//! The zero-padded identifier and the Norway problem — the two failures worth
//! owning code to prevent — YAML 1.2 already prevents. What the table bought
//! beyond that was three spellings no fleet document contains (`1e5`, `0x1F`,
//! `+1`), and the committed fixture corpus uses none of them.
//!
//! It also cost one. The table could not see quote style, so `name: "true"`
//! collapsed to the boolean `true` — a defect this module previously declared
//! and preserved for parity. `yaml_serde` reads it as the string it is, and
//! the two other declared divergences go the same way: a block scalar folds
//! correctly, and an apostrophe in a plain scalar no longer truncates the
//! document, which the pinned `zig-yaml` fork did silently.
//!
//! # What is still ours, because serde does not do it
//!
//! A duplicate key. `yaml_serde` deserialising into a map takes the LAST value
//! silently, and a fleet author who wrote `model:` twice must be told rather
//! than served whichever won. The visitor below is the standard serde answer —
//! it refuses the second insert — not a YAML dialect.
//!
//! Refusing it needs a side channel, because `serde::de::Error::custom` can
//! only carry a STRING and this crate's error must reach the caller with its
//! type intact. [`Refusal`] is that channel: the visitor stashes the typed
//! error and returns whatever serde will take, and [`to_json`] prefers the
//! stashed one. The retired hand-rolled walk did the same thing with a
//! `failure` field, for the same reason.

use std::cell::RefCell;
use std::fmt;

use serde::de::{DeserializeSeed, Deserializer, Error as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

/// Where the visitor leaves a typed refusal for [`to_json`] to pick up.
///
/// `RefCell` rather than `Cell`: [`Error`] is not `Copy`, and the visitor is
/// single-threaded within one `from_str` call, so there is no contention to
/// arbitrate — only interior mutability through a shared reference.
type Refusal = RefCell<Option<Error>>;

/// Renders a frontmatter block as the JSON a fleet's `config_json` stores.
///
/// An empty block is an empty object, matching `yamlFrontmatterToJson`'s
/// `docs.items.len == 0` arm — the document is well-formed and says nothing,
/// which the schema layer above refuses with a sentence naming the block it
/// wanted.
///
/// # Errors
/// Reports YAML this daemon cannot read, a mapping key that is not a scalar,
/// and a key repeated within one mapping.
pub fn to_json(yaml: &str) -> Result<Value> {
    // Checked before the parser rather than after: `yaml_serde` reads a blank
    // block as the null document, and the arm above says an empty block is an
    // empty object.
    if yaml.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let refusal = Refusal::default();
    let outcome = UniqueSeed(&refusal).deserialize(yaml_serde::Deserializer::from_str(yaml));
    // The stashed refusal wins: serde only saw the `Display` of it, and this
    // crate's caller matches on the TYPE.
    if let Some(typed) = refusal.into_inner() {
        return Err(typed);
    }
    Ok(outcome?)
}

/// Delegates every shape to serde except a mapping, which it checks.
struct UniqueVisitor<'a>(&'a Refusal);

impl<'de> Visitor<'de> for UniqueVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a YAML document")
    }

    /// The one arm that is not serde's: a second value for a key is refused.
    ///
    /// `insert` answering `Some` IS the duplicate — there is no prior
    /// `contains_key` read, so no window between the check and the write.
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        let mut entries = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(UniqueSeed(self.0))?;
            if entries.insert(key.clone(), value).is_some() {
                let refusal = Error::DuplicateKey {
                    key: key.into_boxed_str(),
                };
                let rendered = refusal.to_string();
                *self.0.borrow_mut() = Some(refusal);
                return Err(A::Error::custom(rendered));
            }
        }
        Ok(Value::Object(entries))
    }

    /// Carries the duplicate check into a list's elements, so a mapping nested
    /// inside a sequence is checked the way a top-level one is.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(UniqueSeed(self.0))? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_unit<E>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E>(self, v: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> std::result::Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E>(self, v: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(v).map_or_else(
            // JSON holds no NaN and no infinity. Reaching here needs a float
            // `yaml_serde` resolved that JSON cannot store, and the authored
            // text is the only honest answer left.
            || Ok(Value::String(v.to_string())),
            |number| Ok(Value::Number(number)),
        )
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> std::result::Result<Value, E> {
        Ok(Value::String(v))
    }
}

/// Carries [`UniqueVisitor`] into a nested value.
///
/// Without it, serde would deserialize an inner mapping through its own
/// `Value` impl and the duplicate check would apply to the top level only.
struct UniqueSeed<'a>(&'a Refusal);

impl<'de> DeserializeSeed<'de> for UniqueSeed<'_> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Value, D::Error> {
        deserializer.deserialize_any(UniqueVisitor(self.0))
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use super::to_json;
    use crate::Error;

    /// The scalars the retired hand-rolled table existed to defend.
    ///
    /// Kept as a test even though a crate decides them now: these are the
    /// answers this product depends on, and pinning them is what makes a future
    /// parser swap a red suite rather than a silent config change. A
    /// zero-padded channel id becoming an integer is data loss no schema check
    /// downstream would notice.
    #[test]
    fn the_scalars_worth_owning_code_for_are_still_right() {
        // pin test: literal is the contract
        let block = "padded: 01\nchannel: '0123456'\nnorway: NO\nyes_field: yes\non_field: on\n";
        let value = to_json(block).expect("readable");
        assert_eq!(value["padded"], "01", "a leading zero must survive");
        assert_eq!(value["channel"], "0123456");
        assert_eq!(value["norway"], "NO", "the Norway problem must not appear");
        assert_eq!(value["yes_field"], "yes");
        assert_eq!(value["on_field"], "on");
    }

    /// The defect the hand-rolled table carried, now fixed.
    ///
    /// A bare tokeniser cannot see quote style, so `"true"` collapsed to the
    /// boolean. An author who quotes a word means the word.
    #[test]
    fn a_quoted_magic_word_stays_a_string() {
        let value = to_json("q: \"true\"\nn: \"null\"\nd: \"42\"\nbare: true\n").expect("readable");
        assert_eq!(value["q"], "true");
        assert_eq!(value["n"], "null");
        assert_eq!(value["d"], "42");
        assert_eq!(value["bare"], true, "an UNQUOTED magic word still resolves");
    }

    /// A repeated key is refused with its own type, not flattened into the
    /// parser's error — which is what the side channel above exists for.
    #[test]
    fn a_repeated_key_is_refused_by_name() {
        let failure = to_json("name: first\nname: second\n").expect_err("a repeated key");
        assert!(
            matches!(failure, Error::DuplicateKey { ref key } if &**key == "name"),
            "expected a named duplicate, got {failure}"
        );
    }

    /// The check reaches a mapping nested inside a sequence, which is why the
    /// seed carries the channel rather than the top-level call holding it.
    #[test]
    fn a_repeated_key_nested_in_a_list_is_still_refused() {
        let failure = to_json("items:\n  - a: 1\n    a: 2\n").expect_err("a nested duplicate");
        assert!(matches!(failure, Error::DuplicateKey { ref key } if &**key == "a"));
    }

    /// An empty block says nothing and is well-formed; the schema layer above
    /// is what refuses it, with a sentence naming the block it wanted.
    #[test]
    fn an_empty_block_is_an_empty_object() {
        for blank in ["", "   ", "\n\n"] {
            let value = to_json(blank).expect("a blank block is well-formed");
            assert_eq!(value, serde_json::json!({}), "{blank:?}");
        }
    }

    /// Unreadable YAML carries the parser's own message, positioned.
    #[test]
    fn malformed_yaml_reports_where_it_stopped() {
        let failure = to_json("a: [1, 2\nb: broken\n").expect_err("unbalanced flow sequence");
        assert!(matches!(failure, Error::FrontmatterUnreadable { .. }));
        // The cause survives, which is the whole of RUST_ERROR_STANDARD rule 3.
        assert!(std::error::Error::source(&failure).is_some());
    }
}
