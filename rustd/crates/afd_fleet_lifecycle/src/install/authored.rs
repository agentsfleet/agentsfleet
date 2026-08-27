//! The two authored documents, read and made to agree with each other.
//!
//! Split from the choreography next door because it is the half with no I/O in
//! it: everything here is a pure function of the bytes a library entry carried,
//! which is what lets the whole refusal matrix — an unusable `SKILL.md`, an
//! unusable `TRIGGER.md`, the two naming different fleets, tags a lease could
//! not match — be proven without a datastore anywhere near it.

use afd_fleet_runtime::config::Trigger;
use afd_fleet_runtime::{FleetName, ParsedTrigger, SkillMetadata};

use crate::error::{self, ErrorKind, Result};

use super::row::Entry;

/// The most bytes an authored document may carry.
///
/// `markdown_limits.zig`'s value. The refusal SENTENCES say 64KB and this says
/// two hundred; the mismatch is in the Zig too, and it is the NUMBER that is
/// load-bearing — a client is refused at this bound whatever the prose claims.
/// Ported as-is rather than reconciled, because a client already sitting between
/// the two would change class if either moved.
const MAX_MARKDOWN_LEN: usize = 200 * 1024;

/// The daily ceiling a generated `TRIGGER.md` declares.
const DEFAULT_DAILY_DOLLARS: &str = "1.0";

/// The most placement tags a fleet may carry.
const MAX_TAGS: usize = 32;

/// The bytes one placement tag may carry.
const MAX_TAG_LEN: usize = 64;

/// The library entry, with both its documents parsed and cross-checked.
///
/// Carries the [`Entry`] it was read from rather than sitting beside it: every
/// caller that needs one needs the other, and two parameters travelling
/// together through four signatures is the pair asking to be one value.
#[derive(Debug)]
pub(super) struct Authored {
    /// The library row the documents came from.
    pub(super) entry: Entry,
    /// What `SKILL.md` declares — the name, and the placement tags.
    pub(super) skill: SkillMetadata,
    /// The typed policy and the JSON to store, from `TRIGGER.md`.
    pub(super) trigger: ParsedTrigger,
    /// The `TRIGGER.md` text itself, authored or generated.
    pub(super) trigger_markdown: String,
}

impl Authored {
    /// The webhook providers this configuration declares, in document order.
    pub(super) fn webhook_sources(&self) -> Vec<Box<str>> {
        self.trigger
            .config()
            .triggers()
            .iter()
            .filter_map(|declared| match declared {
                Trigger::Webhook(hook) => Some(hook.source.clone()),
                Trigger::Cron(_) | Trigger::Api => None,
            })
            .collect()
    }

    /// The placement tags, as the `TEXT[]` bind wants them.
    pub(super) fn required_tags(&self) -> Vec<String> {
        self.skill.tags().iter().map(ToString::to_string).collect()
    }
}

/// Reads both documents and refuses every way they can disagree.
///
/// A `TRIGGER.md` the bundle did not carry is GENERATED from the skill's name,
/// so a skill-only bundle installs with an API trigger and a default ceiling
/// rather than being refused for a file its author never had to write.
///
/// # Errors
/// Refuses either document being unusable or past its length bound, the two
/// naming different fleets, and placement tags outside what a lease can match.
pub(super) fn read(entry: Entry) -> Result<Authored> {
    let skill_markdown = within_bounds(&entry.skill_markdown, ErrorKind::SkillRejected)?;
    let skill = afd_fleet_runtime::parse_skill(skill_markdown).map_err(error::skill)?;

    let trigger_markdown = match entry.trigger_markdown.as_deref() {
        Some(authored) => within_bounds(authored, ErrorKind::TriggerRejected)?.to_owned(),
        None => generated(skill.name()),
    };
    let trigger = afd_fleet_runtime::parse_trigger(&trigger_markdown)?;

    // One bundle, one identity. Storing either name would make whichever lost a
    // silent lie for as long as the row lives, and the lease path reads both.
    if skill.name().as_str() != trigger.config().name().as_str() {
        return Err(ErrorKind::NameMismatch.into());
    }
    if !tags_fit(skill.tags()) {
        return Err(ErrorKind::RequiredTagsInvalid.into());
    }
    Ok(Authored {
        entry,
        skill,
        trigger,
        trigger_markdown,
    })
}

/// The document, if it is one this daemon will store.
///
/// An empty document and one past the cap are the same refusal, matching
/// `create_fleet_bundle.validateFields`: both mean the entry cannot install.
/// The KIND is passed in rather than derived here, so the caller names its own
/// file and the sentence says which of the two to open.
fn within_bounds(document: &str, rejected: ErrorKind) -> Result<&str> {
    if document.is_empty() || document.len() > MAX_MARKDOWN_LEN {
        return Err(rejected.into());
    }
    Ok(document)
}

/// The `TRIGGER.md` a bundle that carried none installs with.
fn generated(name: &FleetName) -> String {
    format!(
        "---\nname: {}\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: {DEFAULT_DAILY_DOLLARS}\n---\n\n",
        name.as_str()
    )
}

/// Whether the authored placement tags are ones a lease can match on.
///
/// Bounds only — a tag that matches no runner is the author's business, and the
/// runner's label set is not knowable from here. What is refused is a set no
/// lease could evaluate cheaply: `required_tags ⊆ runner.labels` is checked per
/// candidate, so an unbounded set is an unbounded cost on every lease.
fn tags_fit(tags: &[Box<str>]) -> bool {
    tags.len() <= MAX_TAGS
        && tags
            .iter()
            .all(|tag| !tag.is_empty() && tag.len() <= MAX_TAG_LEN)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::{MAX_TAG_LEN, MAX_TAGS, generated, tags_fit};

    #[test]
    fn a_generated_trigger_declares_an_api_wake_and_a_ceiling() {
        let name = afd_fleet_runtime::FleetName::parse("skill-only-install-pin")
            .expect("a kebab slug parses");
        let document = generated(&name);

        assert!(document.contains("name: skill-only-install-pin"));
        assert!(document.contains("type: api"));
        assert!(document.contains("tools: []"));
        assert!(document.contains("daily_dollars: 1.0"));
    }

    #[test]
    fn a_generated_trigger_is_one_this_daemon_can_read_back() {
        // The install stores what this produces and the lease path reads it. A
        // document that generated but did not parse would install a fleet no
        // runner could ever claim.
        let name = afd_fleet_runtime::FleetName::parse("probe").expect("a kebab slug parses");
        let parsed = afd_fleet_runtime::parse_trigger(&generated(&name));

        assert!(parsed.is_ok(), "the generated document must round-trip");
    }

    #[test]
    fn the_tag_bounds_hold_on_both_sides() {
        let one = |text: &str| vec![text.to_owned().into_boxed_str()];

        assert!(tags_fit(&[]), "no tags means any runner");
        assert!(tags_fit(&one(&"a".repeat(MAX_TAG_LEN))));
        assert!(!tags_fit(&one(&"a".repeat(MAX_TAG_LEN + 1))));
        assert!(!tags_fit(&one("")), "an empty tag matches nothing");
        assert!(tags_fit(&vec![one("a").remove(0); MAX_TAGS]));
        assert!(!tags_fit(&vec![one("a").remove(0); MAX_TAGS + 1]));
    }
}
