//! Reading a recorded binding back, and deciding whether it still describes
//! the fleet.
//!
//! The other half of [`Recorded`](super::repository::Recorded). They are one
//! wire shape in two directions — the park writes it onto the gate row, this
//! reads it back when a write mint asks — and a drift between them would let a
//! mint refuse the very binding its own park recorded. Separate files only
//! because the pair would breach the file-length rubric; the contract is the
//! three key names, and both sides name them through the same types.
//!
//! # What "still describes" means, and why it is both directions
//!
//! A human approved a REACH, not a fleet. If the fleet's config has since
//! narrowed, the approval covers more than is now declared and the extra was
//! never re-asked about. If it has WIDENED, the approval covers less than the
//! run would use. Both are drift, so the repository sets must be equal — not
//! one containing the other.
//!
//! Case-insensitively, because GitHub owners and repository names compare that
//! way and a fleet that re-cased its own config did not change its reach.
//! Access and base compare exactly: those are not names, they are decisions.

use serde::Deserialize;

use super::Access;
use super::repository::RepositoryBinding;

/// A recorded binding, as read back off the gate row.
///
/// Mirrors `Recorded`'s three keys. Unknown fields are tolerated rather than
/// refused: a row written by a later build that records a fourth key must not
/// make every earlier binding stop matching, which would refuse work that was
/// legitimately approved.
#[derive(Debug, Deserialize)]
struct Stated {
    /// The repositories the approval covered.
    repositories: Vec<String>,
    /// How far the approved reach went.
    access: Access,
    /// The base the approval opened against, when it had one.
    #[serde(default)]
    base: Option<String>,
}

impl RepositoryBinding {
    /// Does `stated_json` describe exactly this binding?
    ///
    /// Malformed JSON is a MISMATCH, never a pass. Unknown reach must not be
    /// the permissive branch — a row this daemon cannot read is a row it
    /// cannot claim a human approved.
    #[must_use]
    pub fn matches_recorded(&self, stated_json: &str) -> bool {
        let Ok(stated) = serde_json::from_str::<Stated>(stated_json) else {
            return false;
        };
        stated.access == self.access()
            && stated.base.as_deref() == self.base_branch()
            && same_repositories(&stated.repositories, self.repositories())
    }
}

/// Set equality over repository names, case-insensitive and order-insensitive.
///
/// Both directions, which a length check plus one containment would also give
/// — except when a set repeats a name. `["a","a"]` and `["a","b"]` are the
/// same length and every entry of the first is in the second, so a one-
/// directional check would call them equal and admit `b`.
fn same_repositories(stated: &[String], declared: &[Box<str>]) -> bool {
    stated
        .iter()
        .all(|name| contains(declared.iter().map(AsRef::as_ref), name))
        && declared
            .iter()
            .all(|name| contains(stated.iter().map(String::as_str), name))
}

/// Whether `needle` appears in `haystack`, ignoring ASCII case.
fn contains<'a>(mut haystack: impl Iterator<Item = &'a str>, needle: &str) -> bool {
    haystack.any(|candidate| candidate.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use crate::FleetConfig;
    use crate::config::{Mode, RepositoryBinding};
    use crate::provider::StaticRegistry;

    /// The binding a document declares.
    fn binding(declared: &str) -> RepositoryBinding {
        let document = format!(
            r#"{{"name":"probe","x-agentsfleet":{{"triggers":[{{"type":"api"}}],"tools":[],
               "budget":{{"daily_dollars":1.0}},{declared}}}}}"#
        );
        FleetConfig::parse(&document, Mode::Stored, &StaticRegistry::default())
            .expect("a stored document resolves")
            .repository_binding()
            .expect("the document declares a binding")
            .clone()
    }

    /// A write binding over one repository, which is the shape a repair gate
    /// is raised for.
    fn write_binding() -> RepositoryBinding {
        binding(
            r#""repositories":["acme/Payments"],"repository_access":"write","repository_base":"main""#,
        )
    }

    #[test]
    fn the_recorded_binding_matches_the_one_it_was_recorded_from() {
        let declared = write_binding();
        let recorded = serde_json::to_string(&declared.recorded()).expect("the record serialises");

        assert!(declared.matches_recorded(&recorded));
    }

    #[test]
    fn repository_names_compare_without_regard_to_case() {
        // A fleet that re-cased its own config did not change its reach, and
        // GitHub resolves both spellings to one repository.
        let declared = write_binding();

        assert!(declared.matches_recorded(
            r#"{"repositories":["ACME/payments"],"access":"write","base":"main"}"#
        ));
    }

    #[test]
    fn order_does_not_matter_but_membership_does() {
        let declared =
            binding(r#""repositories":["acme/payments","acme/ledger"],"repository_access":"read""#);

        assert!(declared.matches_recorded(
            r#"{"repositories":["acme/ledger","acme/payments"],"access":"read"}"#
        ));
    }

    #[test]
    fn a_config_that_widened_since_the_approval_is_a_mismatch() {
        // The drift this exists to catch: the human approved one repository
        // and the fleet now declares two, so the second was never asked about.
        let declared =
            binding(r#""repositories":["acme/payments","acme/ledger"],"repository_access":"read""#);

        assert!(
            !declared.matches_recorded(r#"{"repositories":["acme/payments"],"access":"read"}"#)
        );
    }

    #[test]
    fn a_config_that_narrowed_since_the_approval_is_also_a_mismatch() {
        // The other direction, and it matters as much: the approval covers a
        // repository the fleet no longer declares, so what was approved is not
        // what would run.
        let declared = binding(r#""repositories":["acme/payments"],"repository_access":"read""#);

        assert!(!declared.matches_recorded(
            r#"{"repositories":["acme/payments","acme/ledger"],"access":"read"}"#
        ));
    }

    #[test]
    fn a_repeated_name_cannot_stand_in_for_a_missing_one() {
        // Equal lengths, and every stated entry appears in the declared set —
        // which a one-directional check would accept, admitting `ledger`.
        let declared =
            binding(r#""repositories":["acme/payments","acme/ledger"],"repository_access":"read""#);

        assert!(!declared.matches_recorded(
            r#"{"repositories":["acme/payments","acme/payments"],"access":"read"}"#
        ));
    }

    #[test]
    fn access_and_base_compare_exactly() {
        let declared = write_binding();

        assert!(
            !declared.matches_recorded(
                r#"{"repositories":["acme/Payments"],"access":"read","base":"main"}"#
            ),
            "a read approval does not authorise a write binding"
        );
        assert!(
            !declared.matches_recorded(
                r#"{"repositories":["acme/Payments"],"access":"write","base":"MAIN"}"#
            ),
            "a base is a decision, not a name — case matters"
        );
        assert!(
            !declared.matches_recorded(r#"{"repositories":["acme/Payments"],"access":"write"}"#),
            "an approval naming no base does not authorise one that does"
        );
    }

    #[test]
    fn a_record_this_daemon_cannot_read_is_a_mismatch() {
        // Never the permissive branch. A row that will not parse is a row this
        // daemon cannot claim a human approved.
        let declared = write_binding();

        for unreadable in [
            "",
            "not json",
            "[]",
            "null",
            r#"{"repositories":"acme/payments","access":"write"}"#,
            r#"{"repositories":["acme/Payments"],"access":"admin"}"#,
            r#"{"access":"write","base":"main"}"#,
        ] {
            assert!(!declared.matches_recorded(unreadable), "{unreadable}");
        }
    }

    #[test]
    fn a_later_builds_extra_key_does_not_stop_a_binding_matching() {
        // Tolerated rather than refused: a fourth key recorded by a newer
        // build must not make every earlier approval stop matching, which
        // would refuse work that WAS approved.
        let declared = write_binding();

        assert!(declared.matches_recorded(
            r#"{"repositories":["acme/Payments"],"access":"write","base":"main","future":1}"#
        ));
    }
}
