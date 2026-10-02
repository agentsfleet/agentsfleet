//! The wire between the supervisor and the executor: JSON-RPC 2.0, one message
//! per line, over the sandbox's Unix socket.
//!
//! The envelopes are `jsonrpsee-types`', and the values the caller sees —
//! [`Ending`], [`Stream`], [`Listing`](crate::api::Listing) — travel as they
//! are, so this module spells only what is the wire's own: the method names,
//! the parameters and results with no caller-side type, and the two
//! notifications a process produces. Bytes travel as standard base64, because
//! output and files need not be text.

use std::borrow::Cow;
use std::collections::BTreeMap;

use base64::prelude::{BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::api::{Ending, Stream};
use crate::error::Result;

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
/// The notification carrying a chunk of a process's output.
pub(crate) const NOTIFY_OUTPUT: &str = "process/output";
/// The notification carrying a process's end; always its last.
pub(crate) const NOTIFY_EXITED: &str = "process/exited";

/// The longest line either end reads. A write of a file is the largest
/// message, and this caps it before base64 expands it by a third.
pub(crate) const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
/// The most a single read answers with, so the reply fits in one frame.
pub(crate) const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// A path that leaves the workspace. Clear of the codes `jsonrpsee-types`
/// reserves, inside the range the specification leaves to servers.
pub(crate) const PATH_REFUSED_CODE: i32 = -32_010;
/// A process this executor does not have, or no longer has.
pub(crate) const UNKNOWN_PROCESS_CODE: i32 = -32_011;

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
    /// The bytes, base64.
    pub(crate) data: String,
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
    /// The bytes read, base64.
    pub(crate) content: String,
    /// Whether the file held more.
    pub(crate) truncated: bool,
}

/// `fs/write` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WriteFileParams<'a> {
    /// The file, inside the workspace.
    pub(crate) path: Cow<'a, str>,
    /// Its new content, base64.
    pub(crate) content: String,
}

/// `fs/list` parameters; the result is a [`Listing`](crate::api::Listing).
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ListParams<'a> {
    /// The directory, inside the workspace.
    pub(crate) path: Cow<'a, str>,
}

/// `process/output` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OutputParams {
    /// The process that wrote it.
    pub(crate) process_id: u64,
    /// Where it wrote it.
    pub(crate) stream: Stream,
    /// The bytes, base64.
    pub(crate) data: String,
}

/// `process/exited` parameters.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExitedParams {
    /// The process that ended.
    pub(crate) process_id: u64,
    /// How it ended.
    pub(crate) ending: Ending,
    /// Output dropped between the kept head and tail.
    pub(crate) omitted_bytes: u64,
}

/// Bytes as the wire carries them.
pub(crate) fn encode(data: &[u8]) -> String {
    BASE64_STANDARD.encode(data)
}

/// Bytes back from the wire.
pub(crate) fn decode(text: &str) -> Result<Bytes> {
    Ok(Bytes::from(BASE64_STANDARD.decode(text)?))
}

/// One message as a line. The wire types serialize infallibly — no map has a
/// non-string key and no `Serialize` impl refuses — so an impossible failure
/// becomes an empty line, which the other end skips as unreadable.
pub(crate) fn line<T: Serialize>(message: &T) -> String {
    serde_json::to_string(message).unwrap_or_default()
}
