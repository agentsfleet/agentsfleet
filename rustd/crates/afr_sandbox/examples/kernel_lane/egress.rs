//! What a sandbox's network reaches under each egress policy, on a real kernel:
//! the host's network, nothing, or exactly its allowlist, held by rules the
//! sandbox cannot touch.

use afr_sandbox::egress_testing::{
    DNS_PORT, FAR_GREETING, FAR_LISTED, FAR_PORT, FAR_UNLISTED, Far, leave, objects,
};
use afr_sandbox::{Engine, Limits, Network, SandboxRequest, probe};
use libtest_mimic::Failed;

use crate::lane::Lane;
use crate::run::{expect, run, runtime, shell};

/// The name the far host is allowlisted under; the sandbox reaches it through
/// its rendered `/etc/hosts` alone.
const FAR_NAME: &str = "far.test";
/// The address every cloud's metadata service answers on, which no allowlist
/// built from names reaches.
const METADATA: &str = "169.254.169.254";
/// A slot no trial's live scope takes: the leftover the sweep must remove.
const LEFTOVER_SLOT: u8 = 200;
/// Connects to `$1:$2` and prints what came back, or `errno <n>`.
const CONNECT: &str = "python3 - \"$@\" <<'EOF'\nimport socket, sys\ntry:\n    \
     s = socket.create_connection((sys.argv[1], int(sys.argv[2])), 3)\n    \
     print(s.recv(16).decode() or 'closed')\nexcept OSError as e:\n    print('errno', e.errno)\nEOF";
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
/// The word `CONNECT` and `FLUSH` print before a failed socket call's number.
const ERRNO: &str = "errno";

/// The command that connects to `host` on `port`.
fn connect(host: &str, port: u16) -> String {
    format!("set -- {host} {port}; {CONNECT}")
}

/// Runs each of `scripts` in one sandbox reaching `network`, and returns what
/// each printed, trimmed.
fn said_under(
    lane: &Lane,
    lease_id: &str,
    network: Network<'_>,
    scripts: &[String],
) -> Result<Vec<String>, Failed> {
    runtime().block_on(async {
        let engine = lane.engine();
        let request = SandboxRequest::new(lease_id, Limits::default()).with_network(network);
        let sandbox = engine.prepare(request).await?;
        let mut said = Vec::with_capacity(scripts.len());
        for script in scripts {
            let outcome = run(sandbox.executor(), shell(script)).await;
            said.push(outcome.map(|outcome| outcome.output.trim().to_owned()));
        }
        sandbox.destroy().await?;
        said.into_iter().collect()
    })
}

/// `allow_all` shares the host's network, so the far host answers; an
/// isolated sandbox has no route to it at all.
pub(crate) fn allow_all_and_deny_all(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let far = connect(&FAR_LISTED.to_string(), FAR_PORT);

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

/// An allowlisted sandbox reaches the listed address by name, and nothing
/// else: not the far host's other address, not the metadata address, and not
/// the resolver port even on the listed address. Its resolver file names no
/// server.
pub(crate) fn allow_list_admits_only_the_set(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    let scripts = [
        connect(FAR_NAME, FAR_PORT),
        connect(&FAR_UNLISTED.to_string(), FAR_PORT),
        connect(METADATA, 80),
        connect(&FAR_LISTED.to_string(), DNS_PORT),
        "grep -c nameserver /etc/resolv.conf || true".to_owned(),
    ];

    let said = said_under(lane, "egress-list", Network::Allowed(&allowlist), &scripts)?;

    let refused = |answer: &String| answer.starts_with(ERRNO);
    expect(
        said.first().is_some_and(|answer| answer == FAR_GREETING)
            && said
                .get(1..4)
                .is_some_and(|answers| answers.iter().all(refused))
            && said.get(4).is_some_and(|count| count == "0"),
        format!("only the listed address answers, by name, with no resolver: {said:?}"),
    )
}

/// The sandbox cannot widen its own rules: a flush sent from inside reaches
/// no rule of its scope, which stays on the host, holding as before.
pub(crate) fn sandbox_cannot_flush_host_rules(lane: &Lane) -> Result<(), Failed> {
    let _far = Far::start()?;
    let allowlist = Far::allowlist(FAR_NAME)?;
    let scripts = [
        FLUSH.to_owned(),
        connect(&FAR_UNLISTED.to_string(), FAR_PORT),
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
pub(crate) fn release_and_boot_sweep(lane: &Lane) -> Result<(), Failed> {
    let allowlist = Far::allowlist(FAR_NAME)?;
    let before = objects()?;
    let during = runtime().block_on(async {
        let engine = lane.engine();
        let request = SandboxRequest::new("egress-release", Limits::default())
            .with_network(Network::Allowed(&allowlist));
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
pub(crate) fn probe_reports_enforcement(lane: &Lane) -> Result<(), Failed> {
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
