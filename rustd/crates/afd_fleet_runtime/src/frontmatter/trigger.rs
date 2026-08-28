//! An authored `TRIGGER.md`, opened into the policy and the JSON beside it.
//!
//! # Why both halves come back
//!
//! `parseTriggerMarkdownWithJson` answers a `ParsedTrigger` carrying the typed
//! config AND the JSON string it was read from, and the install path needs
//! both: the config decides the webhook URLs, the credential set and the cron
//! registration, while the JSON is bound straight into `core.fleets.config_json`
//! as `$6::jsonb`. Re-serializing the config to get the second would be a
//! second spelling of the document, and the two would drift the first time a
//! field gained a default.
//!
//! The markdown BODY is not part of this. The install stores the authored
//! `trigger_markdown` whole and the lease path re-reads the prose from it with
//! [`crate::instructions`], so the body never needs to survive this call.

use serde_json::Value;

use crate::config::FleetConfig;
use crate::error::{Error, Result};

use super::{json, scan};

/// A `TRIGGER.md` read into the two forms the install path needs.
#[derive(Debug, Clone)]
pub struct ParsedTrigger {
    config: FleetConfig,
    config_json: String,
}

impl ParsedTrigger {
    /// The typed policy the document declares.
    #[must_use]
    pub const fn config(&self) -> &FleetConfig {
        &self.config
    }

    /// The JSON the policy was read from — the bytes to store.
    #[must_use]
    pub fn config_json(&self) -> &str {
        &self.config_json
    }

    /// Both halves, for a caller that owns them onward.
    #[must_use]
    pub fn into_parts(self) -> (FleetConfig, String) {
        (self.config, self.config_json)
    }
}

/// Opens an authored `TRIGGER.md`.
///
/// # Errors
/// Reports a document with no frontmatter block, frontmatter that is not
/// readable YAML, and a block whose contents are not a usable fleet
/// configuration. Every one of them reaches a caller as `UZ-AGT-008`.
pub fn parse_trigger(trigger_markdown: &str) -> Result<ParsedTrigger> {
    let block = scan(trigger_markdown).ok_or(Error::FrontmatterMissing)?;
    let config_json = render(&json::to_json(block.yaml())?)?;
    let config = FleetConfig::authored(&config_json)?;
    Ok(ParsedTrigger {
        config,
        config_json,
    })
}

/// The JSON document as the bytes that get stored.
///
/// Separate from the parse so the failure has a name: serializing a tree this
/// module built can only fail on a shape it cannot produce, and saying that
/// once here is cheaper than an `expect` carrying the same argument.
fn render(document: &Value) -> Result<String> {
    Ok(serde_json::to_string(document)?)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::parse_trigger;
    use crate::Error;

    /// The smallest document that installs — the shape of `trigger/minimal.md`.
    const MINIMAL: &str = "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\nProse.\n";

    #[test]
    fn a_minimal_document_yields_a_policy_and_its_json() {
        let parsed = parse_trigger(MINIMAL).expect("a usable document");

        assert_eq!(parsed.config().name().as_str(), "probe");
        assert!(parsed.config_json().contains("x-agentsfleet"));
    }

    #[test]
    fn the_stored_json_re_reads_to_the_same_policy() {
        // The install binds `config_json` and the lease path reads it back, so
        // a document that parses once and not twice would install a fleet no
        // runner could claim.
        let parsed = parse_trigger(MINIMAL).expect("a usable document");
        let reread = crate::FleetConfig::stored(parsed.config_json()).expect("re-readable");

        assert_eq!(reread.name().as_str(), parsed.config().name().as_str());
    }

    #[test]
    fn a_document_with_no_frontmatter_names_the_fence() {
        let failure = parse_trigger("Just prose.\n").expect_err("no block");

        assert!(matches!(failure, Error::FrontmatterMissing));
    }

    #[test]
    fn an_unclosed_block_names_the_fence_rather_than_a_key() {
        // The Zig answers `MissingRequiredField` here, which is advice to add
        // a key to a document whose actual fault is a missing fence.
        let failure = parse_trigger("---\nname: probe\n").expect_err("unclosed");

        assert!(matches!(failure, Error::FrontmatterMissing));
    }

    #[test]
    fn a_block_with_no_runtime_section_is_refused_as_such() {
        let failure = parse_trigger("---\nname: probe\n---\n").expect_err("no runtime block");

        assert!(matches!(failure, Error::RuntimeBlockRequired));
    }
}
