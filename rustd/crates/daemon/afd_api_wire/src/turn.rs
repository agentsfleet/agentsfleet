//! The one change a person makes to a turn: Stop.
//!
//! A `PATCH` of the turn's `status` (HTTP-VERB), on a fleet's turn and a setup
//! draft's alike. The only status a caller may write is `cancelled`, so the
//! field is a one-variant enum: any other word fails to parse, and the refusal
//! is the body's, with no hand-written check beside it.

use serde::{Deserialize, Serialize};

/// The status a caller may set on a turn.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStatusChange {
    /// Stop the turn: cancel it and end the run serving it.
    Cancelled,
}

/// `PATCH …/turns/{turn_id}`: stop the turn.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnPatch {
    /// The status to set; `cancelled` is the only one.
    pub status: TurnStatusChange,
}

#[cfg(test)]
mod tests {
    use super::{TurnPatch, TurnStatusChange};

    #[test]
    fn only_cancelled_parses() {
        let stop: Result<TurnPatch, _> = serde_json::from_str(r#"{"status":"cancelled"}"#);
        assert_eq!(
            stop.ok(),
            Some(TurnPatch {
                status: TurnStatusChange::Cancelled
            })
        );
        for refused in [
            r#"{"status":"answered"}"#,
            r#"{"status":"running"}"#,
            "{}",
            r#"{"status":"cancelled","turn_id":"x"}"#,
        ] {
            assert!(
                serde_json::from_str::<TurnPatch>(refused).is_err(),
                "{refused}"
            );
        }
    }
}
