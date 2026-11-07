//! A native setup a fixture fleet can select, built the way Save builds one.
//!
//! Admission refuses a fleet that selects no setup revision written under the
//! current policy semantics, and the lease path reads its definition. So every
//! suite that seeds a fleet those paths reach seeds one of these beside it.
//! The definition is parsed and digested by the production code here, never
//! typed out as JSON a suite could get subtly wrong (M-TEST-UTIL).
#![expect(
    clippy::expect_used,
    reason = "test seam: a fixture config that does not parse is a fixture bug"
)]

use serde_json::{Value, json};

use crate::definition::{Definition, POLICY_SEMANTICS, canonical_digest};

/// The runtime config a fixture fleet carries when its suite names none: no
/// triggers, no tools, and a budget.
pub const FIXTURE_CONFIG: &str = "{\"name\":\"fixture\",\"x-agentsfleet\":{\"triggers\":[],\"tools\":[],\"budget\":{\"daily_nanos\":\"1000000000\"}}}";

/// What a fixture setup describes itself as.
const FIXTURE_DESCRIPTION: &str = "Runtime fixture.";

/// The instructions a fixture setup carries.
const FIXTURE_INSTRUCTIONS: &str = "Run the fixture.";

/// One parsed setup, ready to store as a revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureSetup {
    /// The definition, as stored JSON.
    pub definition: String,
    /// Its canonical digest.
    pub digest: String,
    /// The runtime config inside it, as JSON text.
    pub config: String,
}

impl FixtureSetup {
    /// The setup for `config`, a runtime config's JSON text.
    ///
    /// # Panics
    /// When `config` is not a valid runtime config.
    #[must_use]
    pub fn new(config: &str) -> Self {
        let config: Value = serde_json::from_str(config).expect("fixture config JSON");
        let document = json!({"schema_version": 1, "description": FIXTURE_DESCRIPTION,
            "instructions_md": FIXTURE_INSTRUCTIONS, "config": config});
        let definition = Definition::parse(&document.to_string()).expect("a valid native setup");
        Self {
            definition: serde_json::to_string(&definition).expect("Definition JSON"),
            digest: canonical_digest(&definition).expect("canonical digest"),
            config: definition.config().value().to_string(),
        }
    }

    /// The policy semantics every fixture setup is written under.
    #[must_use]
    pub const fn semantics(&self) -> &'static str {
        POLICY_SEMANTICS
    }
}

impl Default for FixtureSetup {
    fn default() -> Self {
        Self::new(FIXTURE_CONFIG)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The copy other suites seed through `afd_db` is this setup, byte for byte.
    #[test]
    fn the_seeded_fixture_setup_is_this_one() {
        let setup = FixtureSetup::default();
        assert_eq!(
            setup.definition,
            afd_db::test_util::FIXTURE_SETUP_DEFINITION
        );
        assert_eq!(setup.digest, afd_db::test_util::FIXTURE_SETUP_DIGEST);
        assert_eq!(
            setup.semantics(),
            afd_db::test_util::FIXTURE_SETUP_SEMANTICS
        );
    }

    #[test]
    fn a_fixture_setup_parses_back_and_digests_as_save_would() {
        let setup = FixtureSetup::default();
        let parsed = Definition::parse(&setup.definition).expect("the stored setup parses");
        assert_eq!(canonical_digest(&parsed).expect("digest"), setup.digest);
        assert_eq!(setup.semantics(), POLICY_SEMANTICS);
        assert!(setup.config.contains("\"fixture\""), "{}", setup.config);
    }
}
