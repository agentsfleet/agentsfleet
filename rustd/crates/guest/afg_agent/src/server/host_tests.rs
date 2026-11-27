//! The host's end of the link, for a test: one call written as its line and
//! its answer read back as JSON, with no client of the host's in between.

#![expect(
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the agent"
)]

use std::path::Path;

use af_guest_wire::protocol::request;
use serde::Serialize;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::UnixStream;

/// A connection to an agent bound at a socket, numbering its own calls.
pub(super) struct Host {
    link: BufReader<UnixStream>,
    calls: u64,
}

impl Host {
    /// Connects to the agent bound at `socket`.
    pub(super) async fn connect(socket: &Path) -> Self {
        Self {
            link: BufReader::new(UnixStream::connect(socket).await.unwrap()),
            calls: 0,
        }
    }

    /// Calls `method` with `params` and answers the reply's whole object.
    pub(super) async fn ask(
        &mut self,
        method: &str,
        params: &(impl Serialize + Sync),
    ) -> serde_json::Value {
        self.calls += 1;
        let line = request(self.calls, method, params).unwrap();
        self.link.get_mut().write_all(&line).await.unwrap();
        let mut answer = String::new();
        self.link.read_line(&mut answer).await.unwrap();
        serde_json::from_str(&answer).unwrap()
    }
}
