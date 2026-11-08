//! The trait's file calls refuse by default, all five the same way.

use bytes::Bytes;

use super::{Executor, Process, ProcessId, Spawn};
use crate::error::{Result, file_calls_unsupported};

/// An executor that serves processes and inherits every file call.
#[derive(Debug)]
struct ProcessesOnly;

#[async_trait::async_trait]
impl Executor for ProcessesOnly {
    async fn spawn(&self, _spawn: &Spawn) -> Result<Process> {
        Err(crate::error::unknown_process())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> Result<()> {
        Err(crate::error::unknown_process())
    }

    async fn kill(&self, _process: ProcessId) -> Result<()> {
        Err(crate::error::unknown_process())
    }
}

/// The path every call names; the refusal never reads it.
const PATH: &str = "notes.txt";

#[tokio::test]
async fn an_executor_that_serves_no_files_refuses_every_file_call_alike() {
    let executor = ProcessesOnly;
    let refusals = [
        executor.read_file(PATH, 1).await.err(),
        executor.write_file(PATH, Bytes::new()).await.err(),
        executor.append_file(PATH, Bytes::new()).await.err(),
        executor.delete_file(PATH).await.err(),
        executor.list_dir(PATH).await.err(),
    ];

    let expected = file_calls_unsupported();
    for refused in refusals {
        let refused = refused.map(|failure| (failure.wire_message(), failure.rpc_code()));
        assert_eq!(
            refused,
            Some((expected.wire_message(), expected.rpc_code())),
            "every inherited file call refuses as unsupported"
        );
    }
}
