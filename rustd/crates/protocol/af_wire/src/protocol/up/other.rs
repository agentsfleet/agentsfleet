//! Non-report commands use ordinary tagged decoding; raw report data stays borrowed separately.

use super::{AcceptedKinds, ActivityRequest, FoldRequest, HeartbeatRequest, Renewal, Up};
use serde::Deserialize;
use std::{borrow::Cow, num::NonZeroU64};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Other<
    'a,
    D: Clone = serde_json::Value,
    O: Clone = serde_json::Value,
    I: Clone = serde_json::Value,
> {
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
        observed_offers: u64,
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
        leases: Vec<Renewal<'a>>,
    },
    AuthoringModel {
        request_id: NonZeroU64,
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        request: Cow<'a, crate::authoring::ModelCall<D, O, I>>,
    },
    AuthoringActivity {
        request_id: NonZeroU64,
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        request: Cow<'a, crate::authoring::ActivityRequest<D, O, I>>,
    },
    AuthoringRenew {
        request_id: NonZeroU64,
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        request: crate::authoring::Renew,
    },
    AuthoringReport {
        request_id: NonZeroU64,
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        request: Cow<'a, crate::authoring::Report>,
    },
    Activity {
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        #[serde(borrow)]
        request: ActivityRequest<'a>,
    },
    Folds {
        request_id: NonZeroU64,
        #[serde(borrow)]
        lease_id: Cow<'a, str>,
        request: FoldRequest,
    },
}

// Both enums carry the same fields; forwarding stays exhaustive as variants change.
macro_rules! forward {
    ($value:expr, $($variant:ident { $($field:ident),* }),+ $(,)?) => {
        match $value { $(Other::$variant { $($field),* } => Self::$variant { $($field),* }),+ }
    };
}

impl<'a, D: Clone, O: Clone, I: Clone> From<Other<'a, D, O, I>> for Up<'a, D, O, I> {
    fn from(value: Other<'a, D, O, I>) -> Self {
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
                observed_offers
            },
            Heartbeat {
                request_id,
                request
            },
            Ping { request_id },
            RenewBatch { request_id, leases },
            AuthoringModel {
                request_id,
                lease_id,
                request
            },
            AuthoringActivity {
                request_id,
                lease_id,
                request
            },
            AuthoringRenew {
                request_id,
                lease_id,
                request
            },
            AuthoringReport {
                request_id,
                lease_id,
                request
            },
            Activity { lease_id, request },
            Folds {
                request_id,
                lease_id,
                request
            },
        )
    }
}
