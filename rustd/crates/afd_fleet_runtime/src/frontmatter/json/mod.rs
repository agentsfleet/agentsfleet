//! YAML frontmatter rendered as the JSON the stored schema reads.
//!
//! # This module owns a coercion table, not a YAML dialect
//!
//! `yaml_frontmatter.zig`'s `writeScalar` is the whole of the type system a
//! fleet document has, and it is STRICTER than YAML 1.2 on purpose. Exactly
//! `true`/`false` are booleans, exactly `null`/`~` are null, a scalar passing
//! `is_numeric` is written through as a bare JSON number, and every other
//! scalar — including `True`, `yes`, `on`, `1e5`, `01`, `+1`, `0x1F`, `NaN` —
//! is a JSON string.
//!
//! That table is why the tokeniser under this module resolves nothing. A YAML
//! crate that types scalars for us types them WRONG here and unrecoverably:
//! `01` becomes the integer 1 and `1e5` becomes the float 100000, where this
//! product's answer is the strings `"01"` and `"1e5"`. `saphyr-parser` hands
//! back the authored bytes, so the table below is the only thing deciding, and
//! it can be read against its Zig original line by line.
//!
//! # Three places the Rust and the Zig disagree, all declared
//!
//! 1. **A quoted magic word still collapses.** `name: "true"` renders as the
//!    JSON boolean `true`, not the string `"true"`. The Zig loses quote style
//!    before `writeScalar` sees the scalar and cannot tell the two apart;
//!    `saphyr-parser` DOES hand over the quote style, so this module has to
//!    discard it deliberately to keep the answer. It does, because parity is
//!    this milestone's rule and the corpus grades accept/reject verdicts — the
//!    fix belongs in a milestone that can change both daemons at once.
//!    `a_quoted_magic_word_still_collapses` pins it so the behaviour cannot
//!    drift silently, and deleting that test is the whole cost of fixing it.
//! 2. **Block scalars fold.** `description: |` is a literal block here and a
//!    mis-lexed plain scalar in the pinned `zig-yaml` fork. Reproducing the
//!    Zig would mean writing a known-wrong answer no test could describe.
//! 3. **An apostrophe in a plain scalar no longer truncates the document.**
//!    The fork opens a single-quoted scalar on it and silently drops every key
//!    after — data loss with no error, recorded in M157's Discovery. Here it is
//!    either valid YAML or a refusal with a position.
//!
//! Divergences 2 and 3 are cases where the Zig is wrong and silent. There is no
//! honest way to port a silent wrong answer, so they are fixed and declared.

use saphyr_parser::{Event, EventReceiver, Parser};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

mod scalar;

use self::scalar::scalar_value;

/// Renders a frontmatter block as the JSON a fleet's `config_json` stores.
///
/// An empty block is an empty object, matching `yamlFrontmatterToJson`'s
/// `docs.items.len == 0` arm — the document is well-formed and says nothing,
/// which the schema layer above refuses with a sentence naming the block it
/// wanted.
///
/// Only the FIRST document is read. A second `---`-separated document is
/// ignored rather than refused, because `Yaml.load` indexes `docs.items[0]`
/// and the fence scan above this module has already ended the block at the
/// first closing fence anyway.
///
/// # Errors
/// Reports YAML this daemon cannot tokenise, a mapping key that is not a
/// scalar, and a key repeated within one mapping.
pub fn to_json(yaml: &str) -> Result<Value> {
    let mut build = Build::default();
    Parser::new_from_str(yaml).load(&mut build, true)?;
    build.finish()
}

/// The JSON tree, built as the event stream arrives.
#[derive(Debug, Default)]
struct Build {
    /// The containers currently open, outermost first.
    stack: Vec<Frame>,
    /// The first document's root, once it closes.
    root: Option<Value>,
    /// The first refusal, kept so the walk can run to the end.
    ///
    /// `EventReceiver::on_event` cannot answer a `Result`, so a failure is
    /// recorded and raised by `Build::finish`. First one wins: a later event
    /// reporting a consequence would replace the cause.
    failure: Option<Error>,
}

/// One open container and what it still needs.
#[derive(Debug)]
enum Frame {
    /// A sequence, holding what it has taken so far.
    Sequence(Vec<Value>),
    /// A mapping, plus the key awaiting its value.
    Mapping {
        /// The pairs closed so far, in authored order.
        entries: Map<String, Value>,
        /// The key read but not yet paired.
        pending: Option<String>,
    },
}

impl Build {
    /// The finished root, or the first refusal the walk recorded.
    fn finish(self) -> Result<Value> {
        match self.failure {
            Some(failure) => Err(failure),
            // An empty block is an empty object, not an absent one.
            None => Ok(self.root.unwrap_or_else(|| Value::Object(Map::new()))),
        }
    }

    /// Records the first refusal and lets the walk continue.
    fn refuse(&mut self, failure: Error) {
        self.failure.get_or_insert(failure);
    }

    /// Files one finished value into whatever is open above it.
    fn place(&mut self, value: Value) {
        match self.stack.last_mut() {
            // Nothing open: this is a document root. Only the first is kept.
            None => {
                self.root.get_or_insert(value);
            }
            Some(Frame::Sequence(items)) => items.push(value),
            Some(Frame::Mapping { entries, pending }) => match pending.take() {
                Some(key) => {
                    if entries.insert(key.clone(), value).is_some() {
                        self.refuse(Error::DuplicateKey {
                            key: key.into_boxed_str(),
                        });
                    }
                }
                // A container in key position — `[a]: b`. The Zig's map is
                // keyed by string and cannot hold one either.
                None => self.refuse(Error::NonScalarKey),
            },
        }
    }

    /// Takes a scalar, as a key when one is due and as a value otherwise.
    ///
    /// Keys keep their AUTHORED bytes. Only values go through the coercion
    /// table, which is what makes a mapping key spelled `true` the string
    /// `"true"` on the left of the colon and the boolean on the right — the
    /// Zig's behaviour, for the same reason: its map is keyed by `[]const u8`.
    fn scalar(&mut self, raw: &str) {
        if let Some(Frame::Mapping { pending, .. }) = self.stack.last_mut()
            && pending.is_none()
        {
            *pending = Some(raw.to_owned());
            return;
        }
        self.place(scalar_value(raw));
    }

    /// Closes the innermost container and files it.
    fn close(&mut self) {
        match self.stack.pop() {
            Some(Frame::Sequence(items)) => self.place(Value::Array(items)),
            Some(Frame::Mapping { entries, .. }) => self.place(Value::Object(entries)),
            // The tokeniser pairs its own start and end events; an unmatched
            // end would be a defect in it rather than in the document.
            None => self.refuse(Error::NonScalarKey),
        }
    }
}

impl<'input> EventReceiver<'input> for Build {
    fn on_event(&mut self, ev: Event<'input>) {
        match ev {
            Event::Scalar(raw, _style, _anchor, _tag) => self.scalar(raw.as_ref()),
            Event::SequenceStart(..) => self.stack.push(Frame::Sequence(Vec::new())),
            Event::MappingStart(..) => self.stack.push(Frame::Mapping {
                entries: Map::new(),
                pending: None,
            }),
            Event::SequenceEnd | Event::MappingEnd => self.close(),
            // Stream and document markers carry no value, and aliases are not
            // modelled — the fork's `Value` has no variant for one either.
            _other => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use serde_json::json;

    use super::to_json;

    #[test]
    fn the_coercion_table_matches_write_scalar() {
        let rendered = to_json("t: true\nf: false\nn: null\ntilde: ~\nempty:\ni: 5\nd: 1.25")
            .expect("readable frontmatter");

        assert_eq!(
            rendered,
            json!({"t": true, "f": false, "n": null, "tilde": null,
                   "empty": null, "i": 5, "d": 1.25})
        );
    }

    #[test]
    fn scalars_isnumeric_refuses_are_strings() {
        // Every one of these is a number to YAML 1.2 or to serde, and a STRING
        // to this product. `01` and `1e5` are the two that a resolving YAML
        // crate gets wrong in a way no post-processing recovers.
        let rendered = to_json("a: 01\nb: 1e5\nc: '1.'\nd: .5\ne: +1\nf: 0x1F\ng: '-'")
            .expect("readable frontmatter");

        assert_eq!(
            rendered,
            json!({"a": "01", "b": "1e5", "c": "1.", "d": ".5",
                   "e": "+1", "f": "0x1F", "g": "-"})
        );
    }

    #[test]
    fn a_quoted_magic_word_still_collapses() {
        // DIVERGENCE 1, pinned. `saphyr-parser` hands over the quote style and
        // this module discards it, so `"true"` renders as the boolean the Zig
        // renders. Delete this test when both daemons can change together.
        let rendered = to_json("name: \"true\"\nversion: \"null\"").expect("readable");

        assert_eq!(rendered, json!({"name": true, "version": null}));
    }

    #[test]
    fn a_key_spelled_like_a_magic_word_stays_a_string() {
        // Only VALUES go through the table; the Zig's map is keyed by bytes.
        let rendered = to_json("true: 1\n01: 2").expect("readable frontmatter");

        assert_eq!(rendered, json!({"true": 1, "01": 2}));
    }

    #[test]
    fn nesting_and_sequences_survive_in_authored_order() {
        let rendered = to_json(
            "x-agentsfleet:\n  network:\n    allow: [a, b]\n  tools:\n    - one\n    - two",
        )
        .expect("readable frontmatter");

        assert_eq!(
            rendered,
            json!({"x-agentsfleet": {"network": {"allow": ["a", "b"]},
                                     "tools": ["one", "two"]}})
        );
    }

    #[test]
    fn an_empty_block_is_an_empty_object() {
        assert_eq!(to_json("").expect("readable"), json!({}));
        assert_eq!(to_json("\n").expect("readable"), json!({}));
    }

    #[test]
    fn a_repeated_key_is_refused_by_name() {
        // The pinned `zig-yaml` fork raises `DuplicateMapKey` and
        // `config_markdown.zig` collapses it onto `MissingRequiredField`,
        // which tells an author to add the key they just wrote twice.
        let failure = to_json("name: one\nname: two").expect_err("a duplicate key");

        assert!(matches!(failure, crate::Error::DuplicateKey { ref key } if &**key == "name"));
    }

    #[test]
    fn unreadable_yaml_is_refused_with_its_position() {
        let failure = to_json("a: [1, 2\nb: 3").expect_err("unterminated flow sequence");

        assert!(matches!(
            failure,
            crate::Error::FrontmatterUnreadable { .. }
        ));
    }
}
