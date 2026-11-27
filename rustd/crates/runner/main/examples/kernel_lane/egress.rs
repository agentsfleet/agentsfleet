//! What a guest's tap reaches under its plan, on a real kernel: exactly its
//! allowlist, held by a host table the guest cannot touch, built and removed
//! with the sandbox and swept at the next boot when a runner dies.

use afr_engine::{Engine as _, Limits, Network, Sandbox};
use afr_firewall::far::{FAR_GREETING, FAR_PORT, FAR_UNLISTED, Far, leave, objects};
use libtest_mimic::Failed;

use crate::egress_closed::allow_list_closes_dns_the_host_and_inbound;
use crate::egress_owned::{
    host_cannot_delete_a_live_table, probe_refuses_a_dropping_forward_chain,
};
use crate::egress_refill::{
    reallow_keeps_an_address_both_sets_hold, reallow_swaps_the_set_in_place,
};
use crate::lane::{Lane, id_of, spec};
use crate::run::{expect, in_sandbox_each, run, runtime, said, shell};
use crate::trials::{Body, FAR_NAME};

pub(crate) use afr_engine::conformance::{ERRNO, connect};

/// Every egress trial, by the name the lane reports it under.
pub(crate) const TRIALS: &[(&str, Body)] = &[
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

/// A slot no trial's live scope takes: the leftover the sweep must remove.
const LEFTOVER_SLOT: u8 = 200;
/// Sends `nft flush ruleset` as netlink — a batch deleting every table — from
/// inside the guest, and prints the kernel's answer or the socket's errno.
const FLUSH: &str = "python3 - <<'EOF'\nimport socket, struct\n\
     def msg(kind, flags, seq, family, res):\n    \
     body = struct.pack('=BBH', family, 0, socket.htons(res))\n    \
     return struct.pack('=LHHLL', 16 + len(body), kind, flags, seq, 0) + body\n\
     try:\n    s = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, 12)\n    \
     s.sendto(msg(0x10, 1, 1, 0, 10) + msg(0x0a02, 5, 2, 0, 0) + msg(0x11, 1, 3, 0, 10), (0, 0))\n    \
     print('answer', struct.unpack('=i', s.recv(4096)[16:20])[0])\n\
     except OSError as e:\n    print('errno', e.errno)\nEOF";
/// Where a sweep trial's second boot keeps its state.
const SWEEP_STATE: &str = "/var/tmp";

/// The guest cannot widen its own rules: a flush sent from inside reaches no
/// rule of its scope, which stays on the host, holding as before.
fn sandbox_cannot_flush_host_rules(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    let scripts = [
        FLUSH.to_owned(),
        connect(FAR_UNLISTED, FAR_PORT),
        connect(FAR_NAME, FAR_PORT),
    ];

    let outcomes = in_sandbox_each(
        lane,
        "egress-flush",
        Limits::default(),
        Network::Allowed(&allowlist),
        &scripts,
    )?;
    let said = said(outcomes);

    expect(
        said.first().is_some_and(|answer| answer != "answer 0")
            && said.get(1).is_some_and(|answer| answer.starts_with(ERRNO))
            && said.get(2).is_some_and(|answer| answer == FAR_GREETING),
        format!("the flush changes nothing on the host: {said:?}"),
    )
}

/// A guest's table and link exist while it lives and go with it; a killed
/// runner's leftovers are swept when the next runner boots.
fn release_and_boot_sweep(lane: &Lane) -> Result<(), Failed> {
    let allowlist = Far::allowlist(FAR_NAME)?;
    let before = objects()?;
    let during = runtime().block_on(async {
        let engine = lane.microvms()?;
        let id = id_of("egress-release");
        let request = spec(
            &engine,
            &id,
            Limits::default(),
            Network::Allowed(&allowlist),
        )?;
        let sandbox = engine.prepare(request).await?;
        let during = objects()?;
        sandbox.destroy().await?;
        Ok::<_, Failed>(during)
    })?;
    let after = objects()?;
    leave(LEFTOVER_SLOT)?;
    let left = objects()?;
    let state = tempfile::tempdir_in(SWEEP_STATE)?;
    let booted = lane.boot_over(&lane.config.cgroup_root, state.path())??;
    let swept = objects()?;
    drop(booted.started.engine);
    booted.started.resources.close()?;

    expect(
        during.len() == before.len() + 2,
        format!("a table and a link while it lives: {during:?}"),
    )?;
    expect(after == before, format!("both go with it: {after:?}"))?;
    expect(
        left.len() == before.len() + 2,
        format!("the leftover is made: {left:?}"),
    )?;
    expect(
        swept == before,
        format!("the boot sweep removes it: {swept:?}"),
    )
}

/// The probe a boot measures builds and removes a scope of its own, reports
/// enforcement, and leaves nothing in the host's namespace.
fn probe_reports_enforcement(_lane: &Lane) -> Result<(), Failed> {
    let before = objects()?;

    let enforced = afr_firewall::enforceable();

    expect(
        enforced,
        "a host with nf_tables and forwarding enforces egress",
    )?;
    expect(
        objects()? == before,
        "the probe touched nothing of the host's",
    )
}

/// What each of `scripts` printed in `sandbox`, trimmed, run in turn; the
/// sandbox is destroyed whatever they printed.
pub(crate) async fn said_then_destroyed(
    sandbox: Box<dyn Sandbox>,
    scripts: &[String],
) -> Result<Vec<String>, Failed> {
    let mut said = Vec::with_capacity(scripts.len());
    for script in scripts {
        let outcome = run(sandbox.agent(), shell(script)).await;
        said.push(outcome.map(|outcome| outcome.output.trim().to_owned()));
    }
    sandbox.destroy().await?;
    said.into_iter().collect()
}
