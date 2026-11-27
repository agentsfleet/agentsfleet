//! The wire between the supervisor and the guest agent: JSON-RPC 2.0, one message
//! per line, over the sandbox's Unix socket.
//!
//! The envelopes are `jsonrpsee-types`', and the values the caller sees —
//! [`Ending`], [`Stream`], [`Listing`](crate::Listing) — travel as they
//! are, so this module spells only what is the wire's own: the method names,
//! the parameters and results with no caller-side type, and the two
//! notifications a process produces. Bytes travel as standard base64, because
//! output and files need not be text; they are encoded as the message is
//! written and decoded as it is read, never through a string of their own.

use std::borrow::Cow;
use std::collections::BTreeMap;

use bytes::{BufMut as _, Bytes, BytesMut};
use jsonrpsee_types::{Id, Request};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::{Ending, Stream};

mod pin;
mod transfer;

pub use self::pin::{
    METHOD_PIN_RELEASE, METHOD_PIN_WALK, PinReleaseParams, PinWalkParams, PinWalked,
};
pub use self::transfer::TRANSFER_CHUNK_BYTES;
pub use self::transfer::{
    ExportChunk, ExportOpenParams, ExportReadParams, ImportOpenParams, ImportWriteParams,
    METHOD_EXPORT_OPEN, METHOD_EXPORT_READ, METHOD_IMPORT_COMMIT, METHOD_IMPORT_OPEN,
    METHOD_IMPORT_WRITE, METHOD_TRANSFER_DROP, TransferOpened, TransferParams,
    transfer_chunk_bytes,
};

/// `process/spawn`: start a process.
pub const METHOD_SPAWN: &str = "process/spawn";
/// `process/write`: queue bytes for a process's input.
pub const METHOD_WRITE: &str = "process/write";
/// `process/kill`: end a process's group.
pub const METHOD_KILL: &str = "process/kill";
/// `fs/read`: read a file.
pub const METHOD_READ_FILE: &str = "fs/read";
/// `fs/write`: write a file.
pub const METHOD_WRITE_FILE: &str = "fs/write";
/// `fs/list`: list a directory.
pub const METHOD_LIST_DIR: &str = "fs/list";
/// `fs/delete`: remove a file.
pub const METHOD_DELETE_FILE: &str = "fs/delete";
/// `fs/search`: walk below a path and answer its files or matching lines.
pub const METHOD_SEARCH: &str = "fs/search";
/// An ordered proof that no earlier file operation is still executing.
pub const METHOD_RECOVERY_IDLE: &str = "fs/recovery_idle";
/// `env/renew`: end the inner environment, empty the workspace, build it
/// again.
pub const METHOD_RENEW: &str = "env/renew";
/// `env/hosts`: replace the hosts file every process inside the inner
/// environment resolves names through.
pub const METHOD_HOSTS: &str = "env/hosts";
/// `env/count`: one count the tenant leaf keeps, as the kernel reports it.
pub const METHOD_COUNT: &str = "env/count";
/// `env/freeze`: hold every process in the tenant leaf where it stands.
pub const METHOD_FREEZE: &str = "env/freeze";
/// `env/thaw`: let every process in the tenant leaf run on.
pub const METHOD_THAW: &str = "env/thaw";
/// The notification carrying a chunk of a process's output.
pub const NOTIFY_OUTPUT: &str = "process/output";
/// The notification carrying a process's end; always its last.
pub const NOTIFY_EXITED: &str = "process/exited";

/// The longest line either end reads when its starter names none
/// ([`crate::Sizes`]): 64 MiB.
///
/// A write of a file is the largest message, and
/// this caps it before base64 expands it by a third; a tree crosses in chunks
/// cut to fit it ([`TRANSFER_CHUNK_BYTES`]).
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
/// What ends every message on the wire.
pub const DELIMITER: u8 = b'\n';
/// The most a single read answers with when its starter names none
/// ([`crate::Sizes`]), so the reply fits in one frame. A caller that must
/// have the whole file reads up to it and refuses a file the read cut.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// A path that leaves the workspace. Clear of the codes `jsonrpsee-types`
/// reserves, inside the range the specification leaves to servers.
pub const PATH_REFUSED_CODE: i32 = -32_010;
/// A process this guest agent does not have, or no longer has.
pub const UNKNOWN_PROCESS_CODE: i32 = -32_011;
/// A file or directory the workspace does not have: the one caller's mistake
/// a handler names to the model, so it is told from the rest.
pub const FILE_NOT_FOUND_CODE: i32 = -32_012;
/// A freeze or thaw sent to a guest agent that places no process in a tenant
/// leaf, so a caller can tell it holds nothing from a freeze that failed.
pub const NO_TENANT_LEAF_CODE: i32 = -32_013;
/// `process/spawn` parameters: borrowed where the client sends them, owned
/// where the guest agent reads them.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpawnParams<'a> {
    /// The program, then its arguments.
    pub argv: Cow<'a, [String]>,
    /// Where it starts, relative to the workspace.
    pub cwd: Option<Cow<'a, str>>,
    /// Its whole environment.
    pub env: Cow<'a, BTreeMap<String, String>>,
    /// Whether it runs on a pseudo-terminal.
    pub pty: bool,
    /// When the guest agent kills it.
    pub timeout_ms: Option<u64>,
}

/// `process/spawn` result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpawnResult {
    /// The number later calls name it by.
    pub process_id: u64,
}

/// `process/write` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct WriteParams {
    /// The process to write to.
    pub process_id: u64,
    /// The bytes.
    #[serde(with = "base64_bytes")]
    pub data: Bytes,
}

/// `process/kill` parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct KillParams {
    /// The process to end.
    pub process_id: u64,
}

/// `fs/read` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct ReadParams<'a> {
    /// The file, inside the workspace.
    pub path: Cow<'a, str>,
    /// The most to read; clamped to the guest agent's read size.
    pub max_bytes: u64,
}

/// `fs/read` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    /// The bytes read.
    #[serde(with = "base64_bytes")]
    pub content: Bytes,
    /// Whether the file held more.
    pub truncated: bool,
}

/// `fs/write` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct WriteFileParams<'a> {
    /// The file, inside the workspace.
    pub path: Cow<'a, str>,
    /// Its new content.
    #[serde(with = "base64_bytes")]
    pub content: Bytes,
}

/// `fs/list` and `fs/delete` parameters: one name inside the workspace. A
/// listing answers with a [`Listing`](crate::Listing), a delete with
/// nothing.
#[derive(Debug, Serialize, Deserialize)]
pub struct PathParams<'a> {
    /// The directory or file, inside the workspace.
    pub path: Cow<'a, str>,
}

/// The proof requested before freeze excludes process and provider state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryParams {
    /// Which work the proof inspects.
    pub proof: RecoveryProof,
}

/// `env/renew` parameters: what becomes of the workspace.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenewParams {
    /// What the renewal leaves of the workspace.
    pub workspace: RenewedWorkspace,
}

/// `env/hosts` parameters: the hosts file, whole. The frame is its only
/// bound.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostsParams<'a> {
    /// The hosts file's whole text.
    pub file: Cow<'a, str>,
}

/// `env/count` parameters: which count.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountParams {
    /// Which count.
    pub count: Counted,
}

/// The counts the tenant leaf keeps that the host reads.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Counted {
    /// Forks refused at the leaf's process cap since the guest booted.
    PidsRefused,
}

/// `env/freeze` and `env/thaw` parameters: which leaf.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeafParams {
    /// Which leaf.
    pub leaf: Leaf,
}

/// The leaves a freeze or thaw names: the tenant's, the one leaf the guest agent
/// was given.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Leaf {
    /// The leaf every tenant process is placed in.
    Tenant,
}

/// What a renewal leaves of the workspace: nothing, for a restore to import
/// its tree into.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenewedWorkspace {
    /// Emptied, for a restore to import its tree into.
    Emptied,
}

/// Each supported proof names the agent-owned work it actually inspects.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryProof {
    /// No call runs and no process placed in the tenant leaf remains.
    FileJobs,
    /// No call runs; a tenant process left behind is not counted, since the
    /// pin pause freezes it.
    Calls,
}

/// `process/output` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub struct OutputParams {
    /// The process that wrote it.
    pub process_id: u64,
    /// Where it wrote it.
    pub stream: Stream,
    /// The bytes.
    #[serde(with = "base64_bytes")]
    pub data: Bytes,
}

/// The most one read of output takes.
///
/// The same on pipes and on a terminal, so a noisy process costs the same
/// number of messages either way; the client drops a `process/output` longer
/// than this, which no guest agent sends.
pub const READ_CHUNK_BYTES: usize = 16 * 1024;

/// `process/exited` parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ExitedParams {
    /// The process that ended.
    pub process_id: u64,
    /// How it ended.
    pub ending: Ending,
    /// Whether its output was still open when the drain gave it up, past the
    /// grace or the cap, so what was written after is not read. Absent from
    /// a guest agent that never measured it, which is `false`: a type the
    /// runner reads stays lenient (`af_core::json`).
    #[serde(default)]
    pub output_abandoned: bool,
}

/// One message as a line, ready to write.
///
/// The wire types serialize
/// infallibly — no map has a non-string key and no `Serialize` impl refuses —
/// so an impossible failure becomes an empty line, which the other end skips
/// as unreadable.
pub fn line<T: Serialize>(message: &T) -> Bytes {
    let mut line = BytesMut::new().writer();
    if serde_json::to_writer(&mut line, message).is_err() {
        line.get_mut().clear();
    }
    let mut line = line.into_inner();
    line.put_u8(DELIMITER);
    line.freeze()
}

/// A call to `method` as a line, numbered `id`. Its parameters are encoded
/// once, here, in the caller's task; the link only writes the line.
///
/// # Errors
/// The parameters would not encode.
pub fn request(id: u64, method: &str, params: &impl Serialize) -> serde_json::Result<Bytes> {
    let params = serde_json::value::to_raw_value(params)?;
    Ok(line(&Request::borrowed(
        method,
        Some(&params),
        Id::Number(id),
    )))
}

/// A message's parameters or result, through the object-only gate: whatever
/// is on the other end of the socket is not trusted.
///
/// # Errors
/// The value is not a JSON object, or not the shape `T` names.
pub fn decoded<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> serde_json::Result<T> {
    af_core::json::object_from_slice(raw.get().as_bytes())
}

/// Bytes as standard base64, written straight into the message and read
/// straight out of it.
mod base64_bytes {
    use std::fmt;

    use base64::display::Base64Display;
    use base64::prelude::{BASE64_STANDARD, Engine as _};
    use bytes::Bytes;
    use serde::{Deserializer, Serializer, de};

    pub(super) fn serialize<S: Serializer>(data: &Bytes, wire: S) -> Result<S::Ok, S::Error> {
        wire.collect_str(&Base64Display::new(data, &BASE64_STANDARD))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(wire: D) -> Result<Bytes, D::Error> {
        wire.deserialize_str(Base64)
    }

    /// Decodes the string where it lies in the message.
    struct Base64;

    impl de::Visitor<'_> for Base64 {
        type Value = Bytes;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("standard base64")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Bytes, E> {
            BASE64_STANDARD
                .decode(v)
                .map(Bytes::from)
                .map_err(E::custom)
        }
    }
}

#[cfg(test)]
mod tests;
