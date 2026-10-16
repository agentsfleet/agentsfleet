//! The configuration's published bounds, each read from the constant the
//! parser's own rule reads.
//!
//! Where the parser holds a value to a stronger rule than its garde attribute
//! (a fleet name is a `FleetName`, a channel is a `ChannelId`, a trigger set
//! has a size), the stronger bound is the one published.

use std::num::NonZeroU32;

use utoipa::ToSchema as _;

use super::policy::{FRACTION_MAX, FRACTION_MIN};
use super::{
    AnomalyRule, Budget, Context, Document, GateRule, Gates, MAX_ALLOW_ENTRIES, MAX_ALLOW_LEN,
    MAX_BASE_BRANCH_LEN, MAX_CHANNEL_ID_LEN, MAX_CREDENTIAL_LEN, MAX_CREDENTIALS, MAX_EVENT_LEN,
    MAX_EVENTS, MAX_REFERENCE_LEN, MAX_REPOSITORIES, MAX_REPOSITORY_LEN, MAX_TOOL_LEN, MAX_TOOLS,
    MENTION_CHANNELS, Network, Runtime, Signature, Trigger,
};
use crate::config::anomaly::{MAX_ANOMALY_REPEATS, MAX_ANOMALY_WINDOW_S};
use crate::config::budget::MAX_NANOS_DIGITS;
use crate::config::trigger::{CHANNEL_MIN_LEN, MAX_TRIGGERS};
use crate::name::MAX_NAME_LEN;
use crate::openapi::{Bound, Published, Text};

/// What `afd_validate::rules::ascii_digits` accepts, as a pattern.
const DIGITS: &str = "^[0-9]+$";

/// The fewest entries a list the parser refuses empty may hold.
const NOT_EMPTY: usize = 1;

/// A string the parser refuses empty, up to `max` characters.
const fn required(max: usize) -> Bound {
    Bound::Text(Text::chars(NOT_EMPTY, max))
}

/// A string that may be empty, up to `max` characters.
const fn optional(max: usize) -> Bound {
    Bound::Text(Text::chars(0, max))
}

/// A repository list, keyed and bounded the same wherever one is named.
const REPOSITORIES: (&str, Bound) = (
    "repositories",
    Bound::List {
        min: NOT_EMPTY,
        max: MAX_REPOSITORIES,
        entry: Some(Text::chars(NOT_EMPTY, MAX_REPOSITORY_LEN)),
    },
);

/// A budget ceiling: whole nanos, as decimal digits.
const NANOS: Bound = Bound::Text(Text {
    min: NOT_EMPTY,
    max: MAX_NANOS_DIGITS,
    pattern: Some(DIGITS),
});

/// An anomaly threshold: never zero, never above `cap`.
fn threshold(cap: u32) -> Bound {
    Bound::Range {
        min: NonZeroU32::MIN.get().into(),
        max: cap.into(),
    }
}

/// Every configuration component and the bounds it publishes.
pub(crate) fn published() -> Vec<Published> {
    vec![
        Published {
            component: Document::name,
            closed: true,
            bounds: vec![("name", required(MAX_NAME_LEN))],
        },
        runtime(),
        trigger(),
        Published {
            component: Signature::name,
            closed: true,
            bounds: vec![("secret_ref", required(MAX_CREDENTIAL_LEN))],
        },
        network(),
        Published {
            component: Budget::name,
            closed: true,
            bounds: vec![("daily_nanos", NANOS), ("monthly_nanos", NANOS)],
        },
        Published {
            component: Context::name,
            closed: true,
            bounds: vec![(
                "stage_chunk_threshold",
                Bound::Range {
                    min: FRACTION_MIN.into(),
                    max: FRACTION_MAX.into(),
                },
            )],
        },
        gates(),
    ]
    .into_iter()
    .chain(rules())
    .collect()
}

/// The runtime block's bounds.
fn runtime() -> Published {
    Published {
        component: Runtime::name,
        closed: true,
        bounds: vec![
            (
                "triggers",
                Bound::List {
                    min: 0,
                    max: MAX_TRIGGERS,
                    entry: None,
                },
            ),
            (
                "tools",
                Bound::List {
                    min: 0,
                    max: MAX_TOOLS,
                    entry: Some(Text::chars(NOT_EMPTY, MAX_TOOL_LEN)),
                },
            ),
            (
                "credentials",
                Bound::List {
                    min: 0,
                    max: MAX_CREDENTIALS,
                    entry: Some(Text::chars(NOT_EMPTY, MAX_CREDENTIAL_LEN)),
                },
            ),
            ("model", optional(MAX_REFERENCE_LEN)),
            REPOSITORIES,
            ("repository_base", required(MAX_BASE_BRANCH_LEN)),
        ],
    }
}

/// Every trigger variant's bounds. A key two variants share has one bound.
///
/// Left open: a variant of the internally tagged enum is read from a buffered
/// copy `serde_ignored` never sees, so the parser drops an unknown trigger key
/// rather than refusing it. Closing the schema would publish a refusal the
/// parser does not make; `an_unknown_trigger_key_is_accepted_by_both` pins it.
fn trigger() -> Published {
    Published {
        component: Trigger::name,
        closed: false,
        bounds: vec![
            ("source", required(MAX_REFERENCE_LEN)),
            (
                "events",
                Bound::List {
                    min: NOT_EMPTY,
                    max: MAX_EVENTS,
                    entry: Some(Text::chars(NOT_EMPTY, MAX_EVENT_LEN)),
                },
            ),
            REPOSITORIES,
            ("credential_name", required(MAX_CREDENTIAL_LEN)),
            ("schedule", required(MAX_REFERENCE_LEN)),
            ("timezone", required(MAX_REFERENCE_LEN)),
            ("message", optional(MAX_REFERENCE_LEN)),
            (
                "channels",
                Bound::List {
                    min: MENTION_CHANNELS,
                    max: MENTION_CHANNELS,
                    entry: Some(Text::chars(CHANNEL_MIN_LEN, MAX_CHANNEL_ID_LEN)),
                },
            ),
        ],
    }
}

/// The egress policy's bounds.
fn network() -> Published {
    let entries = Bound::List {
        min: 0,
        max: MAX_ALLOW_ENTRIES,
        entry: Some(Text::chars(NOT_EMPTY, MAX_ALLOW_LEN)),
    };
    Published {
        component: Network::name,
        closed: true,
        bounds: vec![("allow", entries.clone()), ("read_post_paths", entries)],
    }
}

/// The gate block, whose own keys carry no bound beyond their types.
fn gates() -> Published {
    Published {
        component: Gates::name,
        closed: true,
        bounds: Vec::new(),
    }
}

/// The approval and anomaly rules' bounds.
fn rules() -> [Published; 2] {
    [
        Published {
            component: GateRule::name,
            closed: true,
            bounds: vec![
                ("tool", required(MAX_TOOL_LEN)),
                ("action", required(MAX_TOOL_LEN)),
                ("condition", optional(MAX_REFERENCE_LEN)),
                ("gate_kind", optional(MAX_REFERENCE_LEN)),
                ("blast_radius", optional(MAX_REFERENCE_LEN)),
            ],
        },
        Published {
            component: AnomalyRule::name,
            closed: true,
            bounds: vec![
                ("threshold_count", threshold(MAX_ANOMALY_REPEATS)),
                ("threshold_window_s", threshold(MAX_ANOMALY_WINDOW_S)),
            ],
        },
    ]
}
