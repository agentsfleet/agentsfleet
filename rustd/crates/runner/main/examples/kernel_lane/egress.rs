//! What a sandbox's network reaches under each egress policy, on a real kernel:
//! the host's network, nothing, or exactly its allowlist, held by rules the
//! sandbox cannot touch.

use afr_engine::{Engine, Limits, Network};
use afr_engine_bubblewrap::probe;
use afr_firewall::far::{FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far, leave, objects};
use libtest_mimic::Failed;

use crate::egress_closed::allow_list_closes_dns_the_host_and_inbound;
use crate::egress_owned::{
    host_cannot_delete_a_live_table, probe_refuses_a_dropping_forward_chain,
};
use crate::egress_refill::{
    reallow_keeps_an_address_both_sets_hold, reallow_swaps_the_set_in_place,
};
use crate::lane::Lane;
use crate::lane::{id_of, plan, spec};
use crate::run::{expect, in_sandbox_each, runtime, said};
use crate::trials::Body;

pub(crate) use afr_engine::conformance::{ERRNO, connect};

/// Every egress trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
    ("test_kernel_allow_all_and_deny_all", allow_all_and_deny_all),
    (
        "test_kernel_allow_list_closes_dns_the_host_and_inbound",
        allow_list_closes_dns_the_host_and_inbound,
    ),
    (
        "test_kernel_sandbox_cannot_flush_host_rules",
        sandbox_cannot_flush_host_rules,
    ),
    (
        "test_kernel_host_cannot_delete_a_live_table",
        host_cannot_delete_a_live_table,
    ),
    ("test_egress_release_and_boot_sweep", release_and_boot_sweep),
    (
        "test_egress_probe_reports_enforcement",
        probe_reports_enforcement,
    ),
    (
        "test_egress_probe_refuses_a_dropping_forward_chain",
        probe_refuses_a_dropping_forward_chain,
    ),
    (
        "test_kernel_allow_list_refills_in_place",
        reallow_swaps_the_set_in_place,
    ),
    (
        "test_kernel_allow_list_refill_keeps_a_shared_address",
        reallow_keeps_an_address_both_sets_hold,
    ),
];

/// The name the far host is allowlisted under; the sandbox reaches it through
/// its rendered `/etc/hosts` alone.
pub(crate) const FAR_NAME: &str = "far.test";
/// A slot no trial's live scope takes: the leftover the sweep must remove.
const LEFTOVER_SLOT: u8 = 200;
/// Sends `nft flush ruleset` as netlink — a batch deleting every table — from
/// inside the sandbox, and prints the kernel's answer or the socket's errno.
const FLUSH: &str = "python3 - <<'EOF'\nimport socket, struct\n\
     def msg(kind, flags, seq, family, res):\n    \
     body = struct.pack('=BBH', family, 0, socket.htons(res))\n    \
     return struct.pack('=LHHLL', 16 + len(body), kind, flags, seq, 0) + body\n\
     try:\n    s = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, 12)\n    \
     s.sendto(msg(0x10, 1, 1, 0, 10) + msg(0x0a02, 5, 2, 0, 0) + msg(0x11, 1, 3, 0, 10), (0, 0))\n    \
     print('answer', struct.unpack('=i', s.recv(4096)[16:20])[0])\n\
     except OSError as e:\n    print('errno', e.errno)\nEOF";
/// `ENETUNREACH`, as python prints it.
const UNREACHABLE: &str = "errno 101";
/// What each of `scripts` printed, trimmed, run in turn in one sandbox for
/// `run_id` reaching `network`.
fn said_under(
    lane: &Lane,
    run_id: &str,
    network: Network<'_>,
    scripts: &[String],
) -> Result<Vec<String>, Failed> {
    let sandbox_id = id_of(run_id);
    let request = spec(&sandbox_id, Limits::default()).with_egress(plan(1, network));
    in_sandbox_each(lane, request, scripts).map(said)
}

/// `allow_all` shares the host's network, so the far host answers; an
/// isolated sandbox has no route to it at all.
fn allow_all_and_deny_all(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let far = connect(FAR_LISTED, FAR_PORT);

    let host = said_under(
        lane,
        "egress-host",
        Network::Host,
        std::slice::from_ref(&far),
    )?;
    let isolated = said_under(lane, "egress-none", Network::Isolated, &[far])?;

    expect(
        host == [FAR_GREETING],
        format!("allow_all reaches the far host, got {host:?}"),
    )?;
    expect(
        isolated == [UNREACHABLE],
        format!("deny_all reaches nothing, got {isolated:?}"),
    )
}

/// The sandbox cannot widen its own rules: a flush sent from inside reaches
/// no rule of its scope, which stays on the host, holding as before.
fn sandbox_cannot_flush_host_rules(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    let scripts = [
        FLUSH.to_owned(),
        connect(FAR_UNLISTED, FAR_PORT),
        connect(FAR_NAME, FAR_PORT),
    ];

    let said = said_under(lane, "egress-flush", Network::Allowed(&allowlist), &scripts)?;

    expect(
        said.first().is_some_and(|answer| answer != "answer 0")
            && said.get(1).is_some_and(|answer| answer.starts_with(ERRNO))
            && said.get(2).is_some_and(|answer| answer == FAR_GREETING),
        format!("the flush changes nothing on the host: {said:?}"),
    )
}

/// A sandbox's table and link exist while it lives and go with it; a killed
/// run's leftovers are swept when the next engine starts.
fn release_and_boot_sweep(lane: &Lane) -> Result<(), Failed> {
    let allowlist = Far::allowlist(FAR_NAME)?;
    let before = objects()?;
    let during = runtime().block_on(async {
        let engine = lane.engine();
        let sandbox_id = id_of("egress-release");
        let request =
            spec(&sandbox_id, Limits::default()).with_egress(plan(1, Network::Allowed(&allowlist)));
        let sandbox = engine.prepare(request).await?;
        let during = objects()?;
        sandbox.destroy().await?;
        Ok::<_, Failed>(during)
    })?;
    let after = objects()?;
    leave(LEFTOVER_SLOT)?;
    let left = objects()?;
    drop(lane.engine());
    let swept = objects()?;

    expect(
        during.len() == before.len() + 2,
        format!("a table and a link while it lives: {during:?}"),
    )?;
    expect(after == before, format!("both go with it: {after:?}"))?;
    expect(
        left.len() == before.len() + 2,
        format!("the leftover is made: {left:?}"),
    )?;
    expect(swept == before, format!("the sweep removes it: {swept:?}"))
}

/// The probe builds and removes a scope of its own and reports enforcement,
/// and leaves nothing in the host's namespace.
fn probe_reports_enforcement(lane: &Lane) -> Result<(), Failed> {
    let before = objects()?;

    let probed = probe(&lane.config.probe_paths());

    expect(
        probed.egress,
        "a host with nf_tables and forwarding enforces egress",
    )?;
    expect(
        objects()? == before,
        "the probe touched nothing of the host's",
    )
}
