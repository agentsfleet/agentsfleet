//! An MCP server's standard input and output, through the run's
//! environment: one JSON-RPC message per line each way.

use std::sync::Arc;

use bytes::Bytes;
use rmcp::RoleClient;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;

use crate::tools::env::{Env, EnvError, EnvErrorKind, Process, ProcessEvent, Stream};

/// What ends one message.
const NEWLINE: u8 = b'\n';

/// A server process the environment runs, spoken to over its pipes.
#[derive(Debug)]
pub(super) struct EnvStdio {
    env: Arc<dyn Env>,
    process: Process,
    /// What the server wrote that is not yet a whole line.
    pending: Vec<u8>,
}

impl EnvStdio {
    pub(super) fn new(env: Arc<dyn Env>, process: Process) -> Self {
        Self {
            env,
            process,
            pending: Vec::new(),
        }
    }

    /// The next whole line the server wrote, if one is waiting.
    fn line(&mut self) -> Option<Vec<u8>> {
        let end = self.pending.iter().position(|byte| *byte == NEWLINE)?;
        let mut line: Vec<u8> = self.pending.drain(..=end).collect();
        line.pop();
        Some(line)
    }
}

impl Transport<RoleClient> for EnvStdio {
    type Error = EnvError;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let env = Arc::clone(&self.env);
        let id = self.process.id;
        let line = serde_json::to_vec(&item).map(|mut line| {
            line.push(NEWLINE);
            Bytes::from(line)
        });
        async move {
            let line = line
                .map_err(|failure| EnvError::new(EnvErrorKind::Unavailable, failure.to_string()))?;
            env.write(id, line).await
        }
    }

    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        loop {
            while let Some(line) = self.line() {
                // A line that is not a message, such as a server's own log
                // line, is passed over.
                if let Ok(message) = serde_json::from_slice(&line) {
                    return Some(message);
                }
            }
            match self.process.events.recv().await? {
                ProcessEvent::Output {
                    stream: Stream::Stdout | Stream::Terminal,
                    data,
                } => self.pending.extend_from_slice(&data),
                ProcessEvent::Output {
                    stream: Stream::Stderr,
                    ..
                }
                | ProcessEvent::Omitted { .. } => {}
                ProcessEvent::Ended { .. } => return None,
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.env.kill(self.process.id).await
    }
}
