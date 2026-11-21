//! Non-report commands use ordinary tagged decoding; raw report data stays borrowed separately.

use super::{AcceptedKinds, ActivityRequest, FoldRequest, HeartbeatRequest, Renewal, Up};
use serde::Deserialize;
use std::{borrow::Cow, num::NonZeroU64};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Other<'a> {
    Hello {
        request_id: NonZeroU64,
        accepted_kinds: AcceptedKinds,
        #[serde(borrow)]
        running: Vec<Cow<'a, str>>,
        credits: usize,
        #[serde(default, borrow, skip_serializing_if = "Option::is_none")]
        unconfirmed_connection_id: Option<Cow<'a, str>>,
    },
    Confirm {
        request_id: NonZeroU64,
        #[serde(borrow)]
        connection_id: Cow<'a, str>,
    },
    Ready {
        credits: usize,
        revision: NonZeroU64,
        observed_assignments: u64,
    },
    Heartbeat {
        request_id: NonZeroU64,
        #[serde(borrow)]
        request: HeartbeatRequest<'a>,
    },
    Ping {
        request_id: NonZeroU64,
    },
    RenewBatch {
        request_id: NonZeroU64,
        #[serde(borrow)]
        runs: Vec<Renewal<'a>>,
    },
    Progress {
        #[serde(borrow)]
        run_id: Cow<'a, str>,
        #[serde(borrow)]
        request: ActivityRequest<'a>,
    },
    Folds {
        request_id: NonZeroU64,
        #[serde(borrow)]
        run_id: Cow<'a, str>,
        request: FoldRequest,
    },
    TurnCheckpoint {
        request_id: NonZeroU64,
        #[serde(borrow)]
        run_id: Cow<'a, str>,
        request: Cow<'a, crate::run::TurnCheckpointCommand>,
    },
    ContextLoad {
        request_id: NonZeroU64,
        #[serde(borrow)]
        run_id: Cow<'a, str>,
        request: crate::context::ContextLoadRequest,
    },
    ContextSave {
        request_id: NonZeroU64,
        #[serde(borrow)]
        run_id: Cow<'a, str>,
        request: Cow<'a, crate::context::ContextSaveRequest>,
    },
}

// Both enums carry the same fields; forwarding stays exhaustive as variants change.
macro_rules! forward {
    ($value:expr, $($variant:ident { $($field:ident),* }),+ $(,)?) => {
        match $value { $(Other::$variant { $($field),* } => Self::$variant { $($field),* }),+ }
    };
}

impl<'a> From<Other<'a>> for Up<'a> {
    fn from(value: Other<'a>) -> Self {
        forward!(
            value,
            Hello {
                request_id,
                accepted_kinds,
                running,
                credits,
                unconfirmed_connection_id
            },
            Confirm {
                request_id,
                connection_id
            },
            Ready {
                credits,
                revision,
                observed_assignments
            },
            Heartbeat {
                request_id,
                request
            },
            Ping { request_id },
            RenewBatch { request_id, runs },
            Progress { run_id, request },
            Folds {
                request_id,
                run_id,
                request
            },
            TurnCheckpoint {
                request_id,
                run_id,
                request
            },
            ContextLoad {
                request_id,
                run_id,
                request
            },
            ContextSave {
                request_id,
                run_id,
                request
            },
        )
    }
}
