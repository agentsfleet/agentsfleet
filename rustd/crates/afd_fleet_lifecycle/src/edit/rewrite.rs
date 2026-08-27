//! What a PATCH will actually write, derived once from what it sent.
//!
//! Split from the transaction next door because it is the half with no I/O:
//! given the locked row and the request, every column the update binds is a
//! pure function of the two. That is what lets the reparse rules — an unusable
//! document, a replacement `SKILL.md` naming a different fleet, a rename of
//! both documents at once — be proven without a Postgres anywhere near them.

use crate::FleetStatus;
use crate::error::{self, ErrorKind, Result};

use super::{ConfigSource, Patch};

/// The locked row, as it stood before the update.
#[derive(Debug)]
pub(super) struct Snapshot {
    /// The name the row holds now — what a replacement `SKILL.md` is checked
    /// against when the request does not also rename the trigger.
    pub(super) name: String,
    /// Where the fleet stands, which decides what zero updated rows MEANT.
    pub(super) status: FleetStatus,
    /// The stored `SKILL.md`, for the `If-Match` compare and the post-update tag.
    pub(super) source_markdown: String,
    /// The stored `TRIGGER.md`, for the same two.
    pub(super) trigger_markdown: Option<String>,
}

/// Every column the update binds, owned.
///
/// Owned rather than borrowed from the reparsed documents, and that is a
/// deliberate few hundred bytes: borrowing would make this a self-referential
/// struct holding both the parse and the slices into it, which Rust will not
/// build without pinning machinery that buys nothing on a path already doing
/// two round trips.
#[derive(Debug, Default)]
pub(super) struct Rewrite {
    /// The configuration to store, from either source.
    pub(super) config_json: Option<String>,
    /// The `TRIGGER.md` to store, when one was sent.
    pub(super) trigger_markdown: Option<String>,
    /// The `SKILL.md` to store, when one was sent.
    pub(super) source_markdown: Option<String>,
    /// The name to store — set only by a reparsed `TRIGGER.md`.
    pub(super) name: Option<String>,
    /// The placement tags to store, re-derived from a replacement `SKILL.md`.
    pub(super) required_tags: Option<Vec<String>>,
    /// Whether the locked row was already terminal, so zero rows means 404.
    pub(super) was_killed: bool,
}

impl Rewrite {
    /// Reparses whatever the request sent, and refuses what will not store.
    ///
    /// # Errors
    /// Refuses either document being unusable, and a replacement `SKILL.md`
    /// naming a different fleet from the one the row will carry after the write.
    pub(super) fn read(request: &Patch, current: &Snapshot) -> Result<Self> {
        let authored_trigger = match &request.config {
            Some(ConfigSource::Trigger(document)) => Some(document),
            Some(ConfigSource::Json(_sent_directly)) => None,
            None => None,
        };
        let trigger = authored_trigger
            .map(|document| afd_fleet_runtime::parse_trigger(document))
            .transpose()?;
        let skill = request
            .source_markdown
            .as_deref()
            .map(afd_fleet_runtime::parse_skill)
            .transpose()
            .map_err(error::skill)?;

        // The name the row will hold AFTER this write: the reparsed trigger's
        // where one was sent, the stored one otherwise. Checking the
        // replacement `SKILL.md` against that rather than against what is
        // stored now is what makes renaming both documents in one request a
        // legal edit — the Zig does the same, and it is the only reason a
        // rename is possible at all.
        if let Some(replacement) = &skill {
            let target = trigger.as_ref().map_or(current.name.as_str(), |reparsed| {
                reparsed.config().name().as_str()
            });
            if replacement.name().as_str() != target {
                return Err(ErrorKind::NameMismatch.into());
            }
        }

        let (config_json, name) = match (&trigger, &request.config) {
            // A reparsed `TRIGGER.md` writes the configuration AND the name,
            // because the document declares both and storing one without the
            // other would leave the row disagreeing with its own source.
            (Some(reparsed), _) => (
                Some(reparsed.config_json().to_owned()),
                Some(reparsed.config().name().as_str().to_owned()),
            ),
            // A configuration sent directly replaces the stored document and
            // nothing else: the caller sent no name, and inferring one from a
            // JSON blob would rename a fleet nobody asked to rename.
            (None, Some(ConfigSource::Json(document))) => (Some(document.clone()), None),
            (None, _) => (None, None),
        };

        Ok(Self {
            config_json,
            trigger_markdown: authored_trigger.cloned(),
            source_markdown: request.source_markdown.clone(),
            name,
            required_tags: skill
                .as_ref()
                .map(|replacement| replacement.tags().iter().map(ToString::to_string).collect()),
            was_killed: current.status == FleetStatus::Killed,
        })
    }

    /// The `SKILL.md` the row will hold, sent or stored.
    pub(super) fn source_after<'a>(&'a self, current: &'a Snapshot) -> &'a str {
        self.source_markdown
            .as_deref()
            .unwrap_or(&current.source_markdown)
    }

    /// The `TRIGGER.md` the row will hold, sent or stored.
    ///
    /// A request that sends neither keeps whatever is there — the update
    /// `COALESCE`s the column, so the post-update tag has to as well or an
    /// editor's next save would be 412'd against a tag nothing wrote.
    pub(super) fn trigger_after<'a>(&'a self, current: &'a Snapshot) -> Option<&'a str> {
        self.trigger_markdown
            .as_deref()
            .or(current.trigger_markdown.as_deref())
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]
    use super::{Rewrite, Snapshot};
    use crate::edit::{ConfigSource, Patch};
    use crate::{Error, FleetStatus};

    /// A `TRIGGER.md` naming `probe`, the smallest one that parses.
    const TRIGGER: &str = "---\nname: probe\nx-agentsfleet:\n  triggers:\n    - type: api\n  tools: []\n  budget:\n    daily_dollars: 1.0\n---\n";

    /// A `SKILL.md` naming `probe`.
    const SKILL: &str = "---\nname: probe\ndescription: A probe.\nversion: 1.0.0\n---\nProse.\n";

    /// A `SKILL.md` naming something else.
    const SKILL_RENAMED: &str =
        "---\nname: other\ndescription: A probe.\nversion: 1.0.0\n---\nProse.\n";

    fn stored(name: &str) -> Snapshot {
        Snapshot {
            name: name.to_owned(),
            status: FleetStatus::Active,
            source_markdown: SKILL.to_owned(),
            trigger_markdown: Some(TRIGGER.to_owned()),
        }
    }

    fn read(request: &Patch, current: &Snapshot) -> Result<Rewrite, Error> {
        Rewrite::read(request, current)
    }

    #[test]
    fn a_reparsed_trigger_writes_both_the_configuration_and_the_name() {
        let request = Patch {
            config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
            ..Patch::default()
        };
        let rewrite = read(&request, &stored("was-called-this")).expect("a usable document");

        assert_eq!(rewrite.name.as_deref(), Some("probe"));
        assert!(rewrite.config_json.is_some(), "the document declares both");
        assert_eq!(rewrite.trigger_markdown.as_deref(), Some(TRIGGER));
    }

    #[test]
    fn a_configuration_sent_directly_never_renames_the_fleet() {
        // Inferring a name from a JSON blob would rename a fleet on a request
        // that said nothing about its name.
        let request = Patch {
            config: Some(ConfigSource::Json("{\"name\":\"other\"}".to_owned())),
            ..Patch::default()
        };
        let rewrite = read(&request, &stored("probe")).expect("no reparse to fail");

        assert_eq!(rewrite.name, None);
        assert_eq!(rewrite.config_json.as_deref(), Some("{\"name\":\"other\"}"));
        assert_eq!(rewrite.trigger_markdown, None, "the column is untouched");
    }

    #[test]
    fn a_replacement_skill_must_agree_with_the_name_the_row_will_hold() {
        let request = Patch {
            source_markdown: Some(SKILL_RENAMED.to_owned()),
            ..Patch::default()
        };
        let failure = read(&request, &stored("probe")).expect_err("the names disagree");

        assert_eq!(
            failure.code(),
            afd_core::error_code::AGENTSFLEET_NAME_MISMATCH
        );
    }

    #[test]
    fn both_documents_may_be_renamed_in_one_request() {
        // The replacement `SKILL.md` is checked against the name the TRIGGER
        // will write, not against what is stored — otherwise a rename would
        // need two requests and be inconsistent between them.
        let request = Patch {
            config: Some(ConfigSource::Trigger(TRIGGER.to_owned())),
            source_markdown: Some(SKILL.to_owned()),
            ..Patch::default()
        };
        let rewrite = read(&request, &stored("the-old-name")).expect("one coherent rename");

        assert_eq!(rewrite.name.as_deref(), Some("probe"));
        assert!(rewrite.required_tags.is_some(), "tags are re-derived");
    }

    #[test]
    fn a_status_only_patch_leaves_every_document_column_alone() {
        let request = Patch {
            status: Some(crate::Requested::Stopped),
            ..Patch::default()
        };
        let current = stored("probe");
        let rewrite = read(&request, &current).expect("nothing to reparse");

        assert_eq!(rewrite.config_json, None);
        assert_eq!(rewrite.source_markdown, None);
        assert_eq!(rewrite.name, None);
        // And the post-update tag is the stored one, so an open editor is not
        // 412'd by somebody else stopping the fleet.
        assert_eq!(rewrite.source_after(&current), SKILL);
        assert_eq!(rewrite.trigger_after(&current), Some(TRIGGER));
    }

    #[test]
    fn a_killed_row_is_remembered_so_zero_rows_reads_as_gone() {
        let mut current = stored("probe");
        current.status = FleetStatus::Killed;
        let rewrite = read(&Patch::default(), &current).expect("nothing to reparse");

        assert!(rewrite.was_killed);
    }
}
