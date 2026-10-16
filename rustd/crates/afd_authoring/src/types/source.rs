//! Only immutable source identities survive as pins; inline setup is already a candidate.

use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

use super::Source;

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

impl SourcePin {
    pub(crate) fn revision(&self) -> Option<String> {
        match self {
            Self::Library { revision_id, .. } => Some(revision_id.to_string()),
            Self::Fleet { setup_revision, .. } => Some(setup_revision.clone()),
            Self::Blank | Self::Definition => None,
        }
    }
}
