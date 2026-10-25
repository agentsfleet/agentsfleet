//! Reuse the shared canonical cursor codec; bind each walk to its complete scope.

use afd_core::paging::struct_cursor::{self, StructCursor};
use serde::{Deserialize, Serialize};

use super::input::{Bounds, DETAIL_CURSOR};
use crate::handler::Refusal;

/// The query identity, including tenant, cutoff and detail resource.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub tenant: String,
    pub period: String,
    pub fleet_id: Option<String>,
    pub model: Option<String>,
    pub activity_type: Option<String>,
    pub activity_id: Option<String>,
    pub through_at: String,
    pub limit: u32,
}

/// Activity ordering and individual-line ordering have distinct payloads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Position {
    Activity {
        #[garde(skip)]
        at: String,
        #[garde(skip)]
        id: String,
        #[garde(skip)]
        work_type: String,
    },
    Charge {
        #[garde(range(min = 0))]
        sequence: i64,
    },
}

/// One canonical, versioned continuation payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    pub version: u8,
    pub scope: Scope,
    pub position: Position,
}

impl StructCursor for Cursor {
    fn generation(&self) -> u8 {
        self.version
    }
}

impl Cursor {
    /// The input's byte bound must be proved before invoking the shared decoder.
    pub(super) fn parse(raw: &garde::Valid<Bounds>) -> Result<Option<Self>, Refusal> {
        raw.starting_after
            .as_deref()
            .map(|token| {
                let cursor: Self = struct_cursor::parse(token)
                    .map_err(|_foreign| Refusal::malformed(DETAIL_CURSOR))?;
                garde::Unvalidated::new(cursor.position.clone())
                    .validate()
                    .map_err(|_report| Refusal::malformed(DETAIL_CURSOR))?;
                Ok(cursor)
            })
            .transpose()
    }

    pub(super) fn render(scope: &Scope, position: Position) -> String {
        struct_cursor::render(&Self {
            version: struct_cursor::VERSION,
            scope: scope.clone(),
            position,
        })
    }
}
