//! Each notice kind's fixed text, and the key it is owed under.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the restriction set is for the daemon"
)]

use afd_core::id::Uuid7;

use super::{Notice, Subscriber, notice_key, notice_text};

fn subscriber(name: &str, id: &str) -> Subscriber {
    Subscriber {
        fleet: Uuid7::parse(id).expect("a fleet id"),
        name: name.to_owned(),
        runnable: true,
        addressed_only: false,
    }
}

/// Dimension 6.1 — each of the four kinds renders fixed text that names the
/// fleets it is about and the next step, and is owed under one key per event.
#[test]
fn each_notice_kind_owes_one_fixed_text() {
    let responder = subscriber("ci-responder", "0199a0b0-0000-7000-8000-000000000001");
    let repairer = subscriber("ci-repairer", "0199a0b0-0000-7000-8000-000000000002");
    let both = vec![&responder, &repairer];

    for (notice, next_step) in [
        (
            Notice::Ambiguous {
                fleets: both.clone(),
            },
            "exact name",
        ),
        (
            Notice::Choose {
                fleets: both.clone(),
            },
            "Start your message with the name",
        ),
        (
            Notice::AddressIt {
                fleets: both.clone(),
            },
            "answer only when named",
        ),
    ] {
        let text = notice_text(&notice);
        assert!(text.contains("ci-responder, ci-repairer"), "{text}");
        assert!(text.contains(next_step), "{text}");
    }

    let paused = notice_text(&Notice::Paused { fleet: &repairer });
    assert!(paused.starts_with("ci-repairer is paused"), "{paused}");
    assert!(
        paused.contains("`agentsfleet resume 0199a0b0-0000-7000-8000-000000000002`"),
        "the notice names the command, with the fleet filled in: {paused}"
    );

    assert_eq!(notice_key("T024BE7LD", "Ev01"), "T024BE7LD:Ev01:notice");
}
