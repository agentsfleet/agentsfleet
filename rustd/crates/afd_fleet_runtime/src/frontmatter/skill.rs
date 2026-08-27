//! An authored `SKILL.md`, opened into the metadata the install records.
//!
//! # Permissive at the top level, strict about the three it needs
//!
//! `parseSkillMetadata` accepts unknown top-level keys in silence, because a
//! `SKILL.md` is a portable document other skill hosts also read and this
//! daemon has no standing to refuse their vocabulary. What it does insist on
//! is `name`, `description` and `version` — the three the install stores and
//! the dashboard shows — with `name` held to the same kebab slug a fleet name
//! is, and `version` to `MAJOR.MINOR.PATCH`.
//!
//! # Divergence 5, following the one this crate already declared
//!
//! The Zig DROPS a wrong-typed optional and reports nothing: `author: 42`
//! stores no author, `tags: "one"` stores no tags, and the document installs
//! looking fine. [`crate::error`]'s divergence 2 already refused that reading
//! for `skill`, on the grounds that a field silently lost is worse than a field
//! refused with a position. The same rule is applied here rather than kept for
//! one field, because the Zig's own version of it is incoherent — a non-array
//! `tags` is ignored while a non-string ELEMENT of `tags` is a hard error.
//!
//! What is preserved: an EMPTY optional string still reads as absent. That is
//! a normalisation rather than a dropped value, and an author who typed
//! `author: ""` meant the same thing either way.

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::name::{FleetName, Version};

use super::{json, scan};

/// The `name` key, named once for the refusal that cites it.
const FIELD_NAME: &str = "name";

/// The `description` key.
const FIELD_DESCRIPTION: &str = "description";

/// The `version` key.
const FIELD_VERSION: &str = "version";

/// What an authored `SKILL.md` declares about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMetadata {
    name: FleetName,
    description: Box<str>,
    version: Version,
    when_to_use: Option<Box<str>>,
    author: Option<Box<str>>,
    model: Option<Box<str>>,
    tags: Vec<Box<str>>,
}

impl SkillMetadata {
    /// The skill's name, which the install checks against the trigger's.
    #[must_use]
    pub const fn name(&self) -> &FleetName {
        &self.name
    }

    /// The one-line description the dashboard shows.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The authored version.
    #[must_use]
    pub const fn version(&self) -> &Version {
        &self.version
    }

    /// When a human should reach for this skill, if the author said.
    #[must_use]
    pub fn when_to_use(&self) -> Option<&str> {
        self.when_to_use.as_deref()
    }

    /// Who wrote it, if the author said.
    #[must_use]
    pub fn author(&self) -> Option<&str> {
        self.author.as_deref()
    }

    /// The model the skill asks for, if it asks.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// The authored tags, which become the fleet's required tags.
    #[must_use]
    pub fn tags(&self) -> &[Box<str>] {
        &self.tags
    }
}

/// The `SKILL.md` frontmatter as serde reads it.
///
/// Every field is [`Option`] so serde is never asked for a required one and
/// can never raise "missing field" — the crate-wide rule that keeps a MISSING
/// key and a MALFORMED one on separate code paths. Unknown keys are ignored
/// rather than refused, which is the permissive top level.
#[derive(Debug, Deserialize)]
struct Raw {
    /// The skill's name.
    name: Option<String>,
    /// Its one-line description.
    description: Option<String>,
    /// Its version.
    version: Option<String>,
    /// When to reach for it.
    when_to_use: Option<String>,
    /// Who wrote it.
    author: Option<String>,
    /// The model it asks for.
    model: Option<String>,
    /// The tags it declares.
    tags: Option<Vec<String>>,
}

/// Opens an authored `SKILL.md`.
///
/// # Errors
/// Reports a document with no frontmatter block, frontmatter that is not
/// readable YAML, a root that is not a mapping, a missing or empty `name`,
/// `description` or `version`, a name that is not a kebab slug, and a version
/// that is not `MAJOR.MINOR.PATCH`.
pub fn parse_skill(source_markdown: &str) -> Result<SkillMetadata> {
    let block = scan(source_markdown).ok_or(Error::FrontmatterMissing)?;
    let document = json::to_json(block.yaml())?;
    let raw: Raw = serde_json::from_value(document)?;
    resolve(raw)
}

/// Turns the read shape into the checked one.
fn resolve(raw: Raw) -> Result<SkillMetadata> {
    Ok(SkillMetadata {
        name: FleetName::parse(&required(raw.name, FIELD_NAME)?)?,
        description: required(raw.description, FIELD_DESCRIPTION)?.into_boxed_str(),
        version: Version::parse(&required(raw.version, FIELD_VERSION)?)?,
        when_to_use: present(raw.when_to_use),
        author: present(raw.author),
        model: present(raw.model),
        // An absent `tags` is an empty set, not a missing key — the install
        // derives required tags from it and "none declared" is a real answer.
        tags: raw
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(String::into_boxed_str)
            .collect(),
    })
}

/// A required string, refusing both absence and emptiness.
///
/// One refusal for the two because they are the same authoring mistake: a key
/// whose value says nothing is a key that was not filled in.
fn required(value: Option<String>, field: &'static str) -> Result<String> {
    value
        .filter(|found| !found.is_empty())
        .ok_or(Error::MissingRequiredField { field })
}

/// An optional string, with the empty one reading as absent.
fn present(value: Option<String>) -> Option<Box<str>> {
    value
        .filter(|found| !found.is_empty())
        .map(String::into_boxed_str)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::assertions_on_result_states,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::parse_skill;
    use crate::Error;

    /// The three required keys and nothing else — `skill/minimal.md`'s shape.
    const MINIMAL: &str = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\n---\nProse.\n";

    #[test]
    fn the_three_required_keys_are_enough() {
        let skill = parse_skill(MINIMAL).expect("a usable document");

        assert_eq!(skill.name().as_str(), "probe");
        assert_eq!(skill.description(), "A probe.");
        assert_eq!(skill.version().as_str(), "1.0.0");
        assert!(skill.tags().is_empty());
        assert_eq!(skill.author(), None);
    }

    #[test]
    fn every_optional_key_round_trips() {
        let source = "---\nname: probe\ndescription: A probe.\nversion: 1.2.3\nwhen_to_use: Always.\nauthor: Someone\nmodel: a-model\ntags:\n  - one\n  - two\n---\n";
        let skill = parse_skill(source).expect("a usable document");

        assert_eq!(skill.when_to_use(), Some("Always."));
        assert_eq!(skill.author(), Some("Someone"));
        assert_eq!(skill.model(), Some("a-model"));
        assert_eq!(skill.tags().len(), 2);
    }

    #[test]
    fn a_missing_name_names_the_key() {
        let source = "---\ndescription: A probe.\nversion: 1.0.0\n---\n";
        let failure = parse_skill(source).expect_err("no name");

        assert!(matches!(
            failure,
            Error::MissingRequiredField { field: "name" }
        ));
    }

    #[test]
    fn an_empty_required_value_reads_as_missing() {
        let source = "---\nname: probe\ndescription: ''\nversion: 1.0.0\n---\n";
        let failure = parse_skill(source).expect_err("empty description");

        assert!(matches!(
            failure,
            Error::MissingRequiredField {
                field: "description"
            }
        ));
    }

    #[test]
    fn an_empty_optional_value_reads_as_absent() {
        let source = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\nauthor: ''\n---\n";
        let skill = parse_skill(source).expect("a usable document");

        assert_eq!(skill.author(), None);
    }

    #[test]
    fn a_name_that_is_not_a_slug_is_refused() {
        let source = "---\nname: Probe\ndescription: A probe.\nversion: 1.0.0\n---\n";
        let failure = parse_skill(source).expect_err("upper case");

        assert!(matches!(failure, Error::InvalidName { .. }));
    }

    #[test]
    fn a_version_of_the_wrong_arity_is_refused() {
        // Four parts, so `is_numeric` refuses it and it stays a string long
        // enough for `Version::parse` to have an opinion about its arity.
        let source = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0.0\n---\n";
        let failure = parse_skill(source).expect_err("four parts");

        assert!(matches!(failure, Error::InvalidVersion { .. }));
    }

    #[test]
    fn a_two_part_version_refuses_as_a_shape_rather_than_an_arity() {
        // Surprising and correct, and the Zig lands in the same place by the
        // same route: `1.0` passes `is_numeric`, so the converter writes a
        // JSON NUMBER and `version` never reaches the arity check as a string.
        // Quoting does not save it — this module discards quote style
        // (divergence 1), exactly as `writeScalar` does.
        //
        // The Zig's spelling of the refusal is `MissingRequiredField`, which
        // says a key is absent when it is plainly present. Same verdict, and
        // this one names the shape.
        for source in [
            "---\nname: probe\ndescription: A probe.\nversion: 1.0\n---\n",
            "---\nname: probe\ndescription: A probe.\nversion: '1.0'\n---\n",
        ] {
            let failure = parse_skill(source).expect_err("a numeric version");

            assert!(
                matches!(failure, Error::InvalidFieldType { .. }),
                "{source} should refuse as a shape"
            );
        }
    }

    #[test]
    fn unknown_top_level_keys_pass_through() {
        // Other skill hosts add their own vocabulary; refusing it would make
        // this daemon the reason a portable document stops being portable.
        let source =
            "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\nvendor-x: anything\n---\n";

        assert!(parse_skill(source).is_ok());
    }

    #[test]
    fn a_wrong_typed_tag_element_is_refused() {
        // Parity with the Zig's verdict, reached by serde's own shape failure
        // — which names the offending index where `InvalidTagFormat` does not.
        let source =
            "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\ntags: [one, 42]\n---\n";
        let failure = parse_skill(source).expect_err("a numeric tag");

        assert!(matches!(failure, Error::InvalidFieldType { .. }));
    }

    #[test]
    fn a_wrong_typed_tags_key_is_refused_rather_than_dropped() {
        // DIVERGENCE 5. The Zig silently stores no tags here.
        let source = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\ntags: one\n---\n";
        let failure = parse_skill(source).expect_err("a scalar tags");

        assert!(matches!(failure, Error::InvalidFieldType { .. }));
    }
}
