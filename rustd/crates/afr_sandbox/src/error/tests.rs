//! What a sandbox failure says about itself, apart from its code.

use super::{EgressRefusal, cgroup_unreadable, egress_refused};

/// The control file the failure names.
const FILE: &str = "cgroup.events";

#[test]
fn test_detail_is_the_failures_sentence_without_its_code() {
    let failure = cgroup_unreadable(FILE)(std::io::Error::other("gone"));

    let shown = failure.to_string();
    let detail = failure.detail();

    assert_eq!(detail, format!("the cgroup file {FILE} could not be read"));
    assert!(
        shown.starts_with('[') && shown.contains(&detail),
        "the code leads the shown failure and the detail follows: {shown}"
    );
}

/// A log's reason is the failure's sentence, then each cause beneath it, and
/// never the code the log's `error_code` field already carries.
#[test]
fn test_told_is_the_sentence_then_each_cause_without_the_code() {
    let failure = cgroup_unreadable(FILE)(std::io::Error::other("gone"));

    let told = failure.told();

    assert_eq!(told, format!("{}: gone", failure.detail()));
    assert!(!told.contains(failure.code().as_str()), "{told}");
}

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
            EgressRefusal::TooManyAddresses(257),
            "257 addresses, past the 256 a lease may reach",
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
            format!("the lease's egress was refused: {sentence}"),
            "{label}"
        );
        assert_eq!(failure.egress_refusal(), Some(&refusal));
    }
}

/// Each netlink step reads as the phrase an operator has always read after
/// "the kernel refused", whatever value a test tells it apart by.
#[cfg(target_os = "linux")]
#[test]
fn test_each_netlink_step_keeps_its_sentence() {
    use super::{Step, netlink};

    let sentences = [
        (Step::OpenNetfilter, "a netfilter socket"),
        (Step::InstallRules, "the egress table"),
        (Step::OpenRoute, "a route socket"),
        (Step::Join, "the veth pair joining the sandbox to the host"),
        (Step::ConfigurePeer, "the sandbox side of its veth pair"),
        (Step::RemoveRules, "removing the egress table"),
        (Step::RefillRules, "refilling the egress set"),
        (Step::RemoveLink, "removing the veth pair"),
        (Step::ListChains, "listing the host's forward chains"),
        (Step::ListTables, "listing egress tables"),
        (Step::ListLinks, "listing egress links"),
    ];

    for (step, sentence) in sentences {
        let failure = netlink(step)(std::io::Error::from_raw_os_error(libc::EPERM));

        // pin test: literal is the contract
        assert_eq!(failure.detail(), format!("the kernel refused {sentence}"));
        assert_eq!(failure.netlink_step(), Some(step));
    }
}
