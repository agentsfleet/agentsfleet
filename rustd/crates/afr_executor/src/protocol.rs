//! The wire between the supervisor and the executor: JSON-RPC 2.0, one message
//! per line, over the sandbox's Unix socket.
//!
//! The envelopes are `jsonrpsee-types`', and the values the caller sees —
//! [`Ending`], [`Stream`], [`Listing`](crate::api::Listing) — travel as they
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

use crate::api::{Ending, Stream};

/// `process/spawn`: start a process.
pub(crate) const METHOD_SPAWN: &str = "process/spawn";
/// `process/write`: queue bytes for a process's input.
pub(crate) const METHOD_WRITE: &str = "process/write";
/// `process/kill`: end a process's group.
pub(crate) const METHOD_KILL: &str = "process/kill";
/// `fs/read`: read a file.
pub(crate) const METHOD_READ_FILE: &str = "fs/read";
/// `fs/write`: write a file.
pub(crate) const METHOD_WRITE_FILE: &str = "fs/write";
/// `fs/list`: list a directory.
pub(crate) const METHOD_LIST_DIR: &str = "fs/list";
/// `fs/append`: add to the end of a file, making it when absent.
pub(crate) const METHOD_APPEND_FILE: &str = "fs/append";
/// `fs/delete`: remove a file.
pub(crate) const METHOD_DELETE_FILE: &str = "fs/delete";
/// The notification carrying a chunk of a process's output.
pub(crate) const NOTIFY_OUTPUT: &str = "process/output";
/// The notification carrying a process's end; always its last.
pub(crate) const NOTIFY_EXITED: &str = "process/exited";

/// The longest line either end reads. A write of a file is the largest
/// message, and this caps it before base64 expands it by a third.
pub(crate) const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
/// What ends every message on the wire.
pub(crate) const DELIMITER: u8 = b'\n';
/// The most a single read answers with, so the reply fits in one frame. A
/// caller that must have the whole file reads up to it and refuses a file
/// the read cut.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// A path that leaves the workspace. Clear of the codes `jsonrpsee-types`
/// reserves, inside the range the specification leaves to servers.
pub(crate) const PATH_REFUSED_CODE: i32 = -32_010;
/// A process this executor does not have, or no longer has.
pub(crate) const UNKNOWN_PROCESS_CODE: i32 = -32_011;
/// A file or directory the workspace does not have: the one caller's mistake
/// a handler names to the model, so it is told from the rest.
pub(crate) const FILE_NOT_FOUND_CODE: i32 = -32_012;

/// `process/spawn` parameters: borrowed where the client sends them, owned
/// where the executor reads them.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SpawnParams<'a> {
    /// The program, then its arguments.
    pub(crate) argv: Cow<'a, [String]>,
    /// Where it starts, relative to the workspace.
    pub(crate) cwd: Option<Cow<'a, str>>,
    /// Its whole environment.
    pub(crate) env: Cow<'a, BTreeMap<String, String>>,
    /// Whether it runs on a pseudo-terminal.
    pub(crate) pty: bool,
    /// When the executor kills it.
    pub(crate) timeout_ms: Option<u64>,
}

/// `process/spawn` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SpawnResult {
    /// The number later calls name it by.
    pub(crate) process_id: u64,
}

/// `process/write` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WriteParams {
    /// The process to write to.
    pub(crate) process_id: u64,
    /// The bytes.
    #[serde(with = "base64_bytes")]
    pub(crate) data: Bytes,
}

/// `process/kill` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct KillParams {
    /// The process to end.
    pub(crate) process_id: u64,
}

/// `fs/read` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ReadParams<'a> {
    /// The file, inside the workspace.
    pub(crate) path: Cow<'a, str>,
    /// The most to read; clamped to [`MAX_READ_BYTES`].
    pub(crate) max_bytes: u64,
}

/// `fs/read` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReadResult {
    /// The bytes read.
    #[serde(with = "base64_bytes")]
    pub(crate) content: Bytes,
    /// Whether the file held more.
    pub(crate) truncated: bool,
}

/// `fs/write` and `fs/append` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WriteFileParams<'a> {
    /// The file, inside the workspace.
    pub(crate) path: Cow<'a, str>,
    /// Its new content.
    #[serde(with = "base64_bytes")]
    pub(crate) content: Bytes,
}

/// `fs/list` and `fs/delete` parameters: one name inside the workspace. A
/// listing answers with a [`Listing`](crate::api::Listing), a delete with
/// nothing.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PathParams<'a> {
    /// The directory or file, inside the workspace.
    pub(crate) path: Cow<'a, str>,
}

/// `process/output` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OutputParams {
    /// The process that wrote it.
    pub(crate) process_id: u64,
    /// Where it wrote it.
    pub(crate) stream: Stream,
    /// The bytes.
    #[serde(with = "base64_bytes")]
    pub(crate) data: Bytes,
}

/// The most one read of output takes.
///
/// The same on pipes and on a terminal, so a noisy process costs the same
/// number of messages either way; the client drops a `process/output` longer
/// than this, which no executor sends.
pub const READ_CHUNK_BYTES: usize = 16 * 1024;

/// `process/exited` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExitedParams {
    /// The process that ended.
    pub(crate) process_id: u64,
    /// How it ended.
    pub(crate) ending: Ending,
    /// Whether its output was still open when the drain gave it up, past the
    /// grace or the cap, so what was written after is not read. Absent from
    /// an executor that never measured it, which is `false`: a type the
    /// runner reads stays lenient (`afd_core::json`).
    #[serde(default)]
    pub(crate) output_abandoned: bool,
}

/// One message as a line, ready to write. The wire types serialize
/// infallibly — no map has a non-string key and no `Serialize` impl refuses —
/// so an impossible failure becomes an empty line, which the other end skips
/// as unreadable.
pub(crate) fn line<T: Serialize>(message: &T) -> Bytes {
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
pub(crate) fn request(id: u64, method: &str, params: &impl Serialize) -> serde_json::Result<Bytes> {
    let params = serde_json::value::to_raw_value(params)?;
    Ok(line(&Request::borrowed(
        method,
        Some(&params),
        Id::Number(id),
    )))
}

/// A message's parameters or result, through the object-only gate: whatever
/// is on the other end of the socket is not trusted.
pub(crate) fn decoded<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> serde_json::Result<T> {
    afd_core::json::object_from_slice(raw.get().as_bytes())
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
#[path = "protocol/tests.rs"]
mod tests;
