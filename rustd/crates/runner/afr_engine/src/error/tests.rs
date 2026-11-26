//! What a sandbox failure says about itself, apart from its code.

use af_core::error_code::{Coded, ErrorCode};

use super::{EgressRefusal, Error, Failure, egress_refused, refused};

/// Each egress refusal says the sentence an operator has always read for it,
/// byte for byte, whatever value a test or a log line tells it apart by.
#[test]
fn test_each_egress_refusal_keeps_its_sentence() {
    let chains = vec![
        "ip filter FORWARD".to_owned(),
        "inet ufw forward".to_owned(),
    ];
    let sentences = [
        (
            EgressRefusal::ForwardingOff,
            "the host does not forward IPv4 (net.ipv4.ip_forward is not 1)",
        ),
        (
            EgressRefusal::ForwardDropped(chains),
            "a forward chain on the host drops by policy, so no allowlisted connection would \
             pass: ip filter FORWARD, inet ufw forward",
        ),
        (
            EgressRefusal::NoNamespace,
            "no process in the sandbox runs in a network namespace of its own",
        ),
        (
            EgressRefusal::NoSlot,
            "every egress slot on this host is held by a running sandbox",
        ),
        (
            EgressRefusal::HeldElsewhere,
            "another runner process owns this host's egress tables and links",
        ),
        (
            EgressRefusal::NoScope,
            "the sandbox was built to no allowlist, so it has no addresses to replace",
        ),
    ];

    for (refusal, sentence) in sentences {
        let label = refusal.as_str();
        let failure = egress_refused(refusal.clone());

        // pin test: literal is the contract
        assert_eq!(
            failure.detail(),
            format!("the run's egress was refused: {sentence}"),
            "{label}"
        );
        assert_eq!(failure.egress_refusal(), Some(&refusal));
    }
}

/// Each egress refusal is logged under a name of its own, the `refusal` field
/// a filter on `egress_scope_refused` or `egress_probe_failed` tells them
/// apart by.
#[test]
fn test_each_egress_refusal_is_logged_under_its_own_name() {
    let names = [
        (EgressRefusal::ForwardingOff, "forwarding_off"),
        (EgressRefusal::ForwardDropped(Vec::new()), "forward_dropped"),
        (EgressRefusal::NoNamespace, "no_namespace"),
        (EgressRefusal::NoSlot, "no_slot"),
        (EgressRefusal::HeldElsewhere, "held_elsewhere"),
        (EgressRefusal::NoScope, "no_scope"),
    ];

    for (refusal, name) in names {
        // pin test: literal is the contract
        assert_eq!(refusal.as_str(), name, "{refusal:?}");
    }
}

/// An engine's own failure, carrying a seam refusal or not, as a building
/// block's error does.
#[derive(Debug, thiserror::Error)]
#[error("the launcher went away")]
struct Launcher {
    carried: Option<Error>,
}

impl Coded for Launcher {
    fn code(&self) -> ErrorCode {
        af_core::error_code::INTERNAL_OPERATION_FAILED
    }

    fn told(&self) -> String {
        self.to_string()
    }
}

impl Failure for Launcher {
    fn seam(&self) -> Option<&Error> {
        self.carried.as_ref()
    }
}

/// A seam refusal an engine's failure carries is still read as the seam's
/// across the seam, and a failure of the engine's own is told as itself.
#[test]
fn test_an_engine_failure_keeps_the_seam_refusal_it_carries() {
    let missing = Error::engine(Launcher {
        carried: Some(refused("landlock")),
    });
    let own = Error::engine(Launcher { carried: None });

    assert_eq!(missing.missing_mechanism(), Some("landlock"));
    assert_eq!(own.missing_mechanism(), None);
    assert_eq!(own.egress_refusal(), None);
    assert!(!own.is_ambiguous());
    assert_eq!(own.detail(), "the launcher went away");
    assert_eq!(own.told(), "the launcher went away");
    assert!(own.to_string().starts_with("[UZ-"), "{own}");
}
