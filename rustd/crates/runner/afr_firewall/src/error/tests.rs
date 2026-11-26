//! What an egress failure says about itself, apart from its code.

use afr_engine::EgressRefusal;

use super::egress_refused;

/// Each netlink step reads as the phrase an operator has always read after
/// "the kernel refused", whatever value a test tells it apart by.
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
    assert_eq!(
        egress_refused(EgressRefusal::NoSlot).netlink_step(),
        None,
        "a refusal the kernel never made names no step"
    );
}

/// The far host's step, which only the kernel lane raises, reads as its own
/// phrase too.
#[cfg(feature = "test-util")]
#[test]
fn test_the_far_hosts_step_keeps_its_sentence() {
    let failure = super::netlink(super::Step::Far)(std::io::Error::from_raw_os_error(libc::EPERM));

    // pin test: literal is the contract
    assert_eq!(failure.detail(), "the kernel refused the far host's link");
}
