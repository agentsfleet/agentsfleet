//! The replies the fake server can be scripted to give, and the RESP bytes
//! each one writes.
//!
//! Split from `fake_redis.rs` so the server loop and the reply vocabulary
//! each fit the file cap; the vocabulary is what a test names, the loop is
//! what serves it.

use super::resp::Request;

/// The topology question every cluster client asks first.
pub(super) const CMD_CLUSTER: &str = "CLUSTER";

/// What the fake does when a command arrives.
#[derive(Debug, Clone)]
pub(crate) enum Reply {
    /// Write these bytes back. RESP, well-formed or not, as the test chooses.
    Raw(&'static str),
    /// Answer nothing and close the socket. This is a server dying mid-command,
    /// which is what the dropped-connection classification is written for.
    Hangup,
    /// Keep the socket open and never answer this command.
    Silent,
    /// The confirmation a server sends for `SSUBSCRIBE`: a RESP3 push echoing
    /// the channel the client asked for. Built here rather than written
    /// literally because the channel name is the test's, not this file's, and
    /// a push rather than an array because that is what the driver matches to
    /// the command it is waiting on.
    SubscribeAck,
    /// The confirmation for `SUNSUBSCRIBE`, same reasoning.
    UnsubscribeAck,
    /// [`Reply::SubscribeAck`], written only after this long — a node slow to
    /// answer a subscribe — while the connection keeps serving everything
    /// else, pushes included.
    HeldSubscribeAck(std::time::Duration),
    /// The answer to `CLUSTER SLOTS` a cluster client insists on before it
    /// sends anything else: this server owns every slot, at its own port. An
    /// empty hostname tells the driver to keep dialling the address it came
    /// in on. Installed by default so a test scripting one fault does not
    /// have to know the handshake. The same rule answers `CLUSTER SHARDS`,
    /// which is how this crate's per-node walks find the one node to visit.
    /// A bulk string, framed from its payload.
    ///
    /// Its own variant rather than a `Raw` literal carrying its own `$NN`,
    /// because both `INFO` fixtures in this workspace carried one that
    /// disagreed with the bytes after it — `$52` over 57, `$41` over 45. A
    /// short length leaves the client parsing the remainder as the next
    /// reply and the test hanging on a deadline, which reads like a slow
    /// datastore rather than a miscounted fixture. Nothing here counts
    /// bytes by hand any more.
    Bulk(&'static str),
    ClusterSlots,
    /// `INFO cluster` as a server in cluster mode answers it.
    ///
    /// Installed by default on the synthetic `INFO CLUSTER` rule key, so a
    /// test scripting an `INFO` fault for the memory section does not also
    /// answer the cluster probe with memory text.
    InCluster,
    /// `INFO cluster` as a standalone answers it — the seed preflight refuses.
    ///
    /// Keyed on `INFO CLUSTER` and not on `INFO`, because preflight asks two
    /// sections of the same command and the driver's own handshake asks a
    /// third thing entirely; one rule for the whole command cannot tell them
    /// apart.
    NotACluster,
}

/// Builds the `subscribe`/`unsubscribe` confirmation Dragonfly pushes back.
///
/// The trailing count is the number of channels the connection now holds. It is
/// reported as one because nothing in these tests branches on it, and a fixture
/// that tracked it would be modelling server state this file does not have.
pub(super) fn confirmation(kind: &str, channel: &[u8]) -> Vec<u8> {
    // `>` is RESP3's push marker: the driver routes it to the push receiver
    // AND treats it as the reply to the subscribe it is waiting on.
    let mut out = format!(">3\r\n${}\r\n{kind}\r\n${}\r\n", kind.len(), channel.len()).into_bytes();
    out.extend_from_slice(channel);
    out.extend_from_slice(b"\r\n:1\r\n");
    out
}

/// The `smessage` push a sharded subscriber receives for one publish.
pub(super) fn smessage(channel: &str, payload: &str) -> Vec<u8> {
    format!(
        ">3\r\n$8\r\nsmessage\r\n${}\r\n{channel}\r\n${}\r\n{payload}\r\n",
        channel.len(),
        payload.len()
    )
    .into_bytes()
}

/// The subcommand of `CLUSTER` that asks for the shard map.
const CLUSTER_SHARDS: &[u8] = b"SHARDS";

/// The command whose sections preflight reads, the section that says what the
/// server IS, and the synthetic rule key the two make together.
const CMD_INFO: &str = "INFO";
const SECTION_CLUSTER: &[u8] = b"CLUSTER";
pub(super) const RULE_INFO_CLUSTER: &str = "INFO CLUSTER";

/// `INFO cluster` as a server in cluster mode answers it, and as one that is
/// not. Only the field preflight reads is carried, with the header a real
/// section leads with: the rest is a dozen counters no caller here looks at.
///
/// The section is `INFO cluster` and NOT `CLUSTER INFO` — they are different
/// replies, and Dragonfly v1.40.2 names `cluster_enabled` in only this one. A
/// fake that answered the other spelling is what let preflight ship reading a
/// field the real server never puts there.
pub(super) const INFO_CLUSTER_ENABLED: &str = "# Cluster\r\ncluster_enabled:1\r\n";
pub(super) const INFO_CLUSTER_DISABLED: &str = "# Cluster\r\ncluster_enabled:0\r\n";

/// The rule table key one request looks up.
///
/// Every command is keyed by its name, except `INFO`, whose section decides
/// which question is being asked: preflight reads `cluster` and `memory` off
/// the same command and a single rule could not answer both.
pub(super) fn rule_key(request: &Request) -> String {
    if request.name == CMD_INFO
        && request
            .first_argument()
            .eq_ignore_ascii_case(SECTION_CLUSTER)
    {
        return RULE_INFO_CLUSTER.to_owned();
    }
    request.name.clone()
}

/// Frames `payload` as a RESP bulk string, counting it rather than trusting
/// a number written beside it.
pub(super) fn bulk(payload: &str) -> Vec<u8> {
    format!("${}\r\n{payload}\r\n", payload.len()).into_bytes()
}

/// One shard owning slots 0..=16383 at `port` — in the `SLOTS` framing the
/// driver handshakes with, or the `SHARDS` framing this crate walks nodes by.
pub(super) fn cluster_topology(port: u16, subcommand: &[u8]) -> Vec<u8> {
    if subcommand.eq_ignore_ascii_case(CLUSTER_SHARDS) {
        return afd_dragonfly::test_util::cluster_shards_reply(port);
    }
    afd_dragonfly::test_util::cluster_slots_reply(port)
}
