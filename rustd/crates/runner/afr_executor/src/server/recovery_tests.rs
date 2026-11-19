//! A cancelled caller cannot hide a file worker from the recovery boundary.

use super::super::launch::Inherit;
use super::*;

type TestResult<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn query(session: &mut Session, answers: &mut mpsc::UnboundedReceiver<Bytes>) -> TestResult<bool> {
    let frame = crate::protocol::request(
        9,
        METHOD_RECOVERY_IDLE,
        &RecoveryParams {
            proof: crate::protocol::RecoveryProof::FileJobs,
        },
    )?;
    session.dispatch(&frame);
    let body: serde_json::Value = serde_json::from_slice(&answers.try_recv()?)?;
    body.get("result")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "executor did not return a recovery proof".into())
}

#[tokio::test]
async fn a_file_worker_remains_busy_after_its_call_has_left_the_agent() -> TestResult {
    let directory = tempfile::tempdir()?;
    let workspace = Arc::new(Workspace::open(directory.path())?);
    let (outbound, mut answers) = mpsc::unbounded_channel();
    let (output, _events) = mpsc::channel(1);
    let mut session = Session::new(workspace, Arc::new(Inherit), outbound, output);
    assert!(query(&mut session, &mut answers)?);
    let (release, wait) = std::sync::mpsc::channel();
    let (entered, started) = tokio::sync::oneshot::channel();
    session.on_files(Id::Number(2), Ok(()), move |workspace, ()| {
        let _caller_gone = entered.send(());
        wait.recv().map_err(std::io::Error::other)?;
        workspace.write("progress", b"complete")
    });
    started.await?;
    assert!(!query(&mut session, &mut answers)?);
    assert!(!directory.path().join("progress").exists());
    release.send(())?;
    let _finished = session.calls.join_next().await;
    let _unheard_file_reply = answers.try_recv()?;
    assert!(query(&mut session, &mut answers)?);
    assert_eq!(
        std::fs::read(directory.path().join("progress"))?,
        b"complete"
    );
    Ok(())
}
