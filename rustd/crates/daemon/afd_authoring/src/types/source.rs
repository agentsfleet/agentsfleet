//! Only immutable source identities survive as pins; inline setup is already a candidate.

use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

use super::Source;

/// `authoring.drafts.source_type` for a chat started empty.
const BLANK: &str = "blank";
/// See [`BLANK`]: a chat started from an inline definition.
const DEFINITION: &str = "definition";
/// See [`BLANK`]: a chat started from a Library revision.
const LIBRARY: &str = "library";
/// See [`BLANK`]: a chat improving a fleet's numbered revision.
const FLEET: &str = "fleet";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SourcePin {
    Blank,
    Definition,
    Library {
        library_id: String,
        revision_id: Uuid7,
    },
    Fleet {
        fleet_id: Uuid7,
        setup_revision: String,
    },
}

impl From<&Source> for SourcePin {
    fn from(source: &Source) -> Self {
        match source {
            Source::Blank => Self::Blank,
            Source::Definition { .. } => Self::Definition,
            Source::Library {
                library_id,
                revision_id,
            } => Self::Library {
                library_id: library_id.clone(),
                revision_id: revision_id.clone(),
            },
            Source::Fleet {
                fleet_id,
                setup_revision,
            } => Self::Fleet {
                fleet_id: fleet_id.clone(),
                setup_revision: setup_revision.clone(),
            },
        }
    }
}

/// A pin as `authoring.drafts` stores it: its type, and the referenced
/// revisions resolved by join. A reference the schema set NULL, because its
/// card was deleted or its fleet purged, leaves no pin.
pub(crate) struct PinColumns {
    pub(crate) source_type: String,
    pub(crate) library: Option<(String, Uuid7)>,
    pub(crate) fleet: Option<(Uuid7, i64)>,
}

impl SourcePin {
    pub(crate) fn revision(&self) -> Option<String> {
        match self {
            Self::Library { revision_id, .. } => Some(revision_id.to_string()),
            Self::Fleet { setup_revision, .. } => Some(setup_revision.clone()),
            Self::Blank | Self::Definition => None,
        }
    }

    /// The `source_type` this pin is stored under.
    pub(crate) const fn source_type(&self) -> &'static str {
        match self {
            Self::Blank => BLANK,
            Self::Definition => DEFINITION,
            Self::Library { .. } => LIBRARY,
            Self::Fleet { .. } => FLEET,
        }
    }

    /// # Errors
    /// A stored `source_type` this daemon does not write is a stored fault.
    pub(crate) fn from_columns(columns: PinColumns) -> crate::Result<Option<Self>> {
        Ok(match crate::draft::word::<Kind>(&columns.source_type)? {
            Kind::Blank => Some(Self::Blank),
            Kind::Definition => Some(Self::Definition),
            Kind::Library => columns
                .library
                .map(|(library_id, revision_id)| Self::Library {
                    library_id,
                    revision_id,
                }),
            Kind::Fleet => columns.fleet.map(|(fleet_id, revision)| Self::Fleet {
                fleet_id,
                setup_revision: revision.to_string(),
            }),
        })
    }
}

/// The stored `source_type` words, spelled as [`SourcePin`]'s tags.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Blank,
    Definition,
    Library,
    Fleet,
}
