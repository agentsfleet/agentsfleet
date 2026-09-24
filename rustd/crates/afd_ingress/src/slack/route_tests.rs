//! The routing table, row by row, with no datastore.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::id::Uuid7;

use super::{Notice, Route, route};
use crate::slack::Subscriber;

/// Fleet ids for the fixtures, one per subscriber a case builds.
const IDS: [&str; 3] = [
    "019feca5-bc9b-72e8-b71f-e2714f6b0121",
    "019feca5-bc9b-72e8-b71f-e2714f6b0122",
    "019feca5-bc9b-72e8-b71f-e2714f6b0123",
];

/// A subscriber by name, whether it can run, and whether it is addressed-only.
fn subscriber(nth: usize, name: &str, runnable: bool, addressed_only: bool) -> Subscriber {
    Subscriber {
        fleet: Uuid7::parse(IDS.get(nth).expect("a fixture id")).expect("a v7 id"),
        name: name.to_owned(),
        runnable,
        addressed_only,
    }
}

fn reader(nth: usize, name: &str) -> Subscriber {
    subscriber(nth, name, true, false)
}

fn writer(nth: usize, name: &str) -> Subscriber {
    subscriber(nth, name, true, true)
}

fn paused(nth: usize, name: &str) -> Subscriber {
    subscriber(nth, name, false, false)
}

/// Dimension 3.1 — every row of the scenario's §4 table, addressed and not.
#[test]
fn routing_table_is_total() {
    // No subscriber: nothing can be named, so the resident answers.
    assert_eq!(
        route(&[], "incident why did CI fail?"),
        Route::Resident {
            message: "incident why did CI fail?"
        }
    );

    // One read-only subscriber answers either way.
    let one_reader = [reader(0, "responder")];
    assert_eq!(
        route(&one_reader, "responder: why?"),
        Route::Addressed {
            fleet: &one_reader[0],
            message: "why?"
        }
    );
    assert_eq!(
        route(&one_reader, "why did CI fail?"),
        Route::Sole {
            fleet: &one_reader[0],
            message: "why did CI fail?"
        }
    );

    // One write-bound subscriber answers only when named.
    let one_writer = [writer(0, "repairer")];
    assert_eq!(
        route(&one_writer, "repairer open the fix"),
        Route::Addressed {
            fleet: &one_writer[0],
            message: "open the fix"
        }
    );
    assert_eq!(
        route(&one_writer, "open the fix"),
        Route::Notice(Notice::AddressIt {
            fleets: vec![&one_writer[0]]
        })
    );

    // Several eligible: the named one, or a notice listing who can be named.
    let several = [reader(0, "responder"), reader(1, "reviewer")];
    assert_eq!(
        route(&several, "Reviewer, look at this"),
        Route::Addressed {
            fleet: &several[1],
            message: "look at this"
        }
    );
    assert_eq!(
        route(&several, "look at this"),
        Route::Notice(Notice::Choose {
            fleets: vec![&several[0], &several[1]]
        })
    );

    // A paused fleet named earns a notice; unnamed, it is not eligible.
    let with_paused = [paused(0, "responder")];
    assert_eq!(
        route(&with_paused, "responder why?"),
        Route::Notice(Notice::Paused {
            fleet: &with_paused[0]
        })
    );
    assert_eq!(
        route(&with_paused, "why?"),
        Route::Notice(Notice::AddressIt {
            fleets: vec![&with_paused[0]]
        })
    );
}

/// The drill's channel: a read-only responder and a write-bound repairer. An
/// unaddressed mention reaches the responder alone; naming the repairer
/// reaches it alone.
#[test]
fn the_drill_channel_routes_by_eligibility_not_by_count() {
    let channel = [reader(0, "ci-dev-responder"), writer(1, "ci-dev-repairer")];
    assert_eq!(
        route(&channel, "why did this fail?"),
        Route::Sole {
            fleet: &channel[0],
            message: "why did this fail?"
        }
    );
    assert_eq!(
        route(&channel, "ci-dev-repairer open the fix"),
        Route::Addressed {
            fleet: &channel[1],
            message: "open the fix"
        }
    );
}

/// Dimension 3.2 — two subscribers whose names differ only in case make that
/// word ambiguous, and neither is ever picked.
#[test]
fn case_folded_duplicates_never_route() {
    let twins = [reader(0, "Incident"), reader(1, "incident")];
    for text in ["incident why?", "INCIDENT: why?", "Incident, why?"] {
        assert_eq!(
            route(&twins, text),
            Route::Notice(Notice::Ambiguous {
                fleets: vec![&twins[0], &twins[1]]
            }),
            "`{text}` names two fleets"
        );
    }
}

/// Dimension 3.3 — a write-bound subscriber is never `Sole`, even alone, and
/// never counted toward one when a read-only fleet shares the channel.
#[test]
fn write_bound_fleets_take_addressed_mentions_only() {
    let alone = [writer(0, "repairer")];
    assert!(
        !matches!(route(&alone, "fix it"), Route::Sole { .. }),
        "a write-bound fleet alone still takes addressed mentions only"
    );
    let two_writers = [writer(0, "repairer"), writer(1, "patcher")];
    assert_eq!(
        route(&two_writers, "fix it"),
        Route::Notice(Notice::AddressIt {
            fleets: vec![&two_writers[0], &two_writers[1]]
        })
    );
}

/// Dimension 3.4 — the addressed name, and the space after it, are removed;
/// the rest is kept byte for byte, internal spacing and punctuation included.
#[test]
fn addressed_name_is_stripped_from_the_message() {
    let one = [reader(0, "responder")];
    for (text, expected) in [
        ("responder: why  did\tit fail?", "why  did\tit fail?"),
        (
            "  responder,   open https://github.com/a/b/actions/runs/1",
            "open https://github.com/a/b/actions/runs/1",
        ),
        ("responder", ""),
    ] {
        assert_eq!(
            route(&one, text),
            Route::Addressed {
                fleet: &one[0],
                message: expected
            },
            "`{text}`"
        );
    }
}

/// An empty mention names nobody, and the terminators alone name nobody.
#[test]
fn an_empty_or_bare_punctuation_mention_is_unaddressed() {
    let one = [reader(0, "responder")];
    for text in ["", "   ", ":", ","] {
        assert!(
            matches!(route(&one, text), Route::Sole { .. }),
            "`{text}` names nobody"
        );
    }
}

/// Every notice kind spells itself apart, for the operator event and the key.
#[test]
fn every_notice_kind_spells_itself_apart() {
    let fleet = reader(0, "responder");
    let kinds = [
        Notice::Ambiguous { fleets: vec![] }.kind(),
        Notice::Choose { fleets: vec![] }.kind(),
        Notice::AddressIt { fleets: vec![] }.kind(),
        Notice::Paused { fleet: &fleet }.kind(),
    ];
    let distinct: std::collections::BTreeSet<&str> = kinds.into_iter().collect();
    assert_eq!(distinct.len(), kinds.len(), "{kinds:?}");
}
