//! The words an item's record carries about itself: what kind of thing the
//! call was and how it ended. They are stored as `thread.items.kind` and
//! `thread.items.status`, spelled by [`ItemKind::as_str`] and
//! [`ItemStatus::as_str`], with no schema check.

use serde::{Deserialize, Serialize};

/// What kind of thing one item of a run is.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// A tool call that changed no file's lines.
    ToolCall,
    /// A call to a tool that edits files: its record names each file and
    /// the lines it added and removed.
    Edit,
}

impl ItemKind {
    /// The kind as it is stored.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ToolCall => "tool_call",
            Self::Edit => "edit",
        }
    }
}

/// How an item's call ended. A record is written once its call answered, so
/// it is never running.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// The call answered with no error and no non-zero exit.
    Succeeded,
    /// The call answered with an error, or its process exited non-zero.
    Failed,
}

impl ItemStatus {
    /// The status as it is stored.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "a test asserts by panicking")]

    use serde_json::json;

    use super::{ItemKind, ItemStatus};

    #[test]
    fn item_words_are_spelled_as_they_are_stored() {
        for (kind, spelled) in [(ItemKind::ToolCall, "tool_call"), (ItemKind::Edit, "edit")] {
            assert_eq!(kind.as_str(), spelled);
            assert_eq!(serde_json::to_value(kind).expect("encodes"), json!(spelled));
        }
        for (status, spelled) in [
            (ItemStatus::Succeeded, "succeeded"),
            (ItemStatus::Failed, "failed"),
        ] {
            assert_eq!(status.as_str(), spelled);
            assert_eq!(
                serde_json::to_value(status).expect("encodes"),
                json!(spelled)
            );
        }
    }
}
