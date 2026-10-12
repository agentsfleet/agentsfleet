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
    /// The name the row holds now, which may carry an install suffix its
    /// documents do not.
    pub(super) name: String,
    /// Where the fleet stands. Read by the caller when zero rows come back,
    /// to tell a tombstone from a refused transition.
    pub(super) status: FleetStatus,
    /// The stored `SKILL.md`, for the `If-Match` compare and the post-update tag.
    pub(super) source_markdown: String,
    /// The stored `TRIGGER.md`, for the same two.
    pub(super) trigger_markdown: Option<String>,
}

impl Snapshot {
    /// The name the stored documents carry, which a sent `TRIGGER.md` must
    /// change for an edit to rename the fleet.
    ///
    /// The stored `TRIGGER.md` declares it. A row without one, or with one
    /// this daemon no longer parses, falls back to the row's own name: the
    /// edit then renames exactly when the sent name differs from the row's.
    fn authored_name(&self) -> String {
        self.trigger_markdown
            .as_deref()
            .and_then(|stored| afd_fleet_runtime::parse_trigger(stored).ok())
            .map_or_else(
                || self.name.clone(),
                |parsed| parsed.config().name().as_str().to_owned(),
            )
    }
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
    /// The credentials the configuration WILL declare, when this write replaces
    /// it.
    ///
    /// Carried out of the parse rather than reparsed later: the document is
    /// already open here, and a second parse is a second chance for the two to
    /// disagree about what the row is going to say.
    pub(super) declared_credentials: Option<Vec<String>>,
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

        // The documents' name before and after this write. The row's name is
        // not it: an install whose name was taken draws a suffixed one
        // (`install/row.rs`), so `probe-b` can hold documents naming `probe`.
        let authored_before = current.authored_name();
        let authored_after = trigger
            .as_ref()
            .map_or(authored_before.as_str(), |reparsed| {
                reparsed.config().name().as_str()
            });

        // A replacement `SKILL.md` is checked against the name the documents
        // will carry after this write, so renaming both in one request is a
        // legal edit and a suffixed fleet's own `SKILL.md` is not refused.
        if let Some(replacement) = &skill
            && replacement.name().as_str() != authored_after
        {
            return Err(ErrorKind::NameMismatch.into());
        }

        let (config_json, name) = match (&trigger, &request.config) {
            // A reparsed `TRIGGER.md` writes the configuration, and the name
            // only when its name line changed: an edit to tools or triggers
            // keeps the name the fleet was installed under.
            (Some(reparsed), _) => (
                Some(reparsed.config_json().to_owned()),
                (authored_after != authored_before).then(|| authored_after.to_owned()),
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
            declared_credentials: trigger.as_ref().map(|reparsed| {
                reparsed
                    .config()
                    .credentials()
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            }),
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
mod tests;
