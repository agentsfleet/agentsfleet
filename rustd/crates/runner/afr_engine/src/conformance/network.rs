//! What a sandbox's network reaches: nothing when it is isolated, and only
//! its listed host, by name, when it holds an allowlist.

use std::fmt;

use libtest_mimic::Failed;

use super::Context;
use super::run::{REACH_OUT, UNREACHABLE, build, build_reaching, expect, said, within};
use crate::engine::{EgressPlan, Engine, Generation, Limits};
use crate::network::{Allowlist, Network};

/// Connects to `$1:$2` over TCP and prints what came back, or `errno <n>`.
pub(super) const CONNECT: &str = "python3 - \"$@\" <<'EOF'\nimport socket, sys\ntry:\n    \
     s = socket.create_connection((sys.argv[1], int(sys.argv[2])), 3)\n    \
     print(s.recv(16).decode() or 'closed')\nexcept OSError as e:\n    print('errno', e.errno)\nEOF";
/// Sends one datagram to `$1:$2` and prints the answer, `timeout`, or
/// `errno <n>`.
pub(super) const DATAGRAM: &str = "python3 - \"$@\" <<'EOF'\nimport socket, sys\n\
     s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)\ns.settimeout(2)\ntry:\n    \
     s.sendto(b'q', (sys.argv[1], int(sys.argv[2])))\n    print(s.recv(16).decode())\n\
     except socket.timeout:\n    print('timeout')\nexcept OSError as e:\n    \
     print('errno', e.errno)\nEOF";
/// The word [`CONNECT`] and [`DATAGRAM`] print before a failed socket call's
/// number.
pub const ERRNO: &str = "errno";
/// What [`DATAGRAM`] prints when nothing answers.
pub const NO_ANSWER: &str = "timeout";
/// Counts the resolvers the sandbox's resolver file names.
pub(super) const RESOLVERS: &str = "grep -c nameserver /etc/resolv.conf || true";
/// The resolver port, which every plan closes even on a listed address.
const DNS_PORT: u16 = 53;
/// QUIC's port, dialled over UDP.
const QUIC_PORT: u16 = 443;
/// The metadata service every cloud answers on, and the port it serves.
pub(super) const METADATA: &str = "169.254.169.254";
const METADATA_PORT: u16 = 80;
/// A name no allowlist lists, so no lookup of it resolves.
const UNLISTED_NAME: &str = "unlisted.invalid";
/// A private address, a carrier-grade one, and a private one spelled as
/// IPv6: each is reserved, so no plan reaches it.
const PRIVATE: &str = "10.0.0.5";
const CARRIER_GRADE: &str = "100.64.0.1";
const MAPPED_PRIVATE: &str = "::ffff:10.0.0.1";

/// The command that connects to `host` on `port` over TCP.
pub fn connect(host: impl fmt::Display, port: u16) -> String {
    format!("set -- {host} {port}; {CONNECT}")
}

/// The command that sends one datagram to `host` on `port`.
pub fn datagram(host: impl fmt::Display, port: u16) -> String {
    format!("set -- {host} {port}; {DATAGRAM}")
}

/// An isolated sandbox has no route out.
pub(super) async fn isolated_reaches_nothing(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let said = within(build(engine, Limits::DEFAULT).await?, async |sandbox| {
        said(sandbox, REACH_OUT).await
    })
    .await?;
    expect(said == UNREACHABLE, format!("no route out, got {said:?}"))
}

/// An allowlisted sandbox reaches its listed host by name, and nothing else:
/// not the host's unlisted address, an unlisted name, a private, link-local,
/// carrier-grade or IPv6 destination, the resolver port on the listed
/// address, or QUIC over UDP to it; and its resolver file names no server.
pub(super) async fn verdict_matrix(
    engine: &dyn Engine,
    context: &Context<'_>,
) -> Result<(), Failed> {
    let far = context.far;
    let allowlist = Allowlist::new(vec![(far.name.to_owned(), far.listed)])?;
    let plan = EgressPlan {
        generation: Generation(1),
        network: Network::Allowed(&allowlist),
    };
    let refused = [
        ("the unlisted address", connect(far.unlisted, far.port)),
        ("an unlisted name", connect(UNLISTED_NAME, far.port)),
        (PRIVATE, connect(PRIVATE, far.port)),
        (METADATA, connect(METADATA, METADATA_PORT)),
        (CARRIER_GRADE, connect(CARRIER_GRADE, far.port)),
        (MAPPED_PRIVATE, connect(MAPPED_PRIVATE, far.port)),
        ("the listed resolver port", connect(far.listed, DNS_PORT)),
        (
            "QUIC to the listed address",
            datagram(far.listed, QUIC_PORT),
        ),
    ];
    let (listed, unlisted, resolvers) =
        within(build_reaching(engine, plan).await?, async |sandbox| {
            let listed = said(sandbox, &connect(far.name, far.port)).await?;
            let mut unlisted = Vec::with_capacity(refused.len());
            for (_label, dial) in &refused {
                unlisted.push(said(sandbox, dial).await?);
            }
            Ok((listed, unlisted, said(sandbox, RESOLVERS).await?))
        })
        .await?;
    expect(
        listed == far.greeting,
        format!("the listed host answers by name, got {listed:?}"),
    )?;
    let reached: Vec<(&str, &String)> = refused
        .iter()
        .zip(&unlisted)
        .filter(|(_dial, answer)| !(answer.starts_with(ERRNO) || *answer == NO_ANSWER))
        .map(|((label, _dial), answer)| (*label, answer))
        .collect();
    expect(
        reached.is_empty(),
        format!("no unlisted destination answers, got {reached:?}"),
    )?;
    expect(
        resolvers == "0",
        format!("the resolver file names no server, got {resolvers:?}"),
    )
}
