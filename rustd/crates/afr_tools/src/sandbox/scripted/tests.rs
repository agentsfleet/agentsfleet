#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afr_executor::{Ending, Executor, ProcessId, Spawn};
use bytes::Bytes;

use super::{NO_SCRIPT, PROCESSES_ONLY, ScriptedExecutor};

/// What the executor says of a process it does not hold, as the double now
/// says it too.
const UNKNOWN_PROCESS: &str = "no process with that identifier";

/// An id no scripted process was given.
const NOBODY: ProcessId = ProcessId::new(7);

#[tokio::test]
async fn should_refuse_file_calls_a_spawn_past_its_scripts_and_unknown_processes() {
    let executor = ScriptedExecutor::new([]);

    let refusals = [
        executor.read_file("a", 1).await.unwrap_err(),
        executor.write_file("a", Bytes::new()).await.unwrap_err(),
        executor.list_dir("a").await.unwrap_err(),
        executor.spawn(&Spawn::program("sh")).await.unwrap_err(),
        executor.write(NOBODY, Bytes::new()).await.unwrap_err(),
        executor.kill(NOBODY).await.unwrap_err(),
    ];

    let said: Vec<String> = refusals
        .iter()
        .map(afr_executor::Error::wire_message)
        .collect();
    let expected = [
        PROCESSES_ONLY,
        PROCESSES_ONLY,
        PROCESSES_ONLY,
        NO_SCRIPT,
        UNKNOWN_PROCESS,
        UNKNOWN_PROCESS,
    ];
    for (message, detail) in said.iter().zip(expected) {
        assert!(message.ends_with(detail), "{message:?} names {detail:?}");
    }
    assert!(!executor.end(NOBODY, Ending::Exited(0)));
    assert!(!executor.forget(NOBODY));
    assert_eq!(executor.killed(), [NOBODY], "an asked kill is recorded");
}
