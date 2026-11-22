//! A durable run's sandbox is never held: when its history cannot be saved
//! the task is blocked, and the sandbox is killed for its release.

use super::*;
use crate::run_loop::hold::{Fate, Kept};
use af_wire::task::{DecisionTaskVersion, TaskReport};

/// The fixture's run, ended with history to save and a waiting task.
fn ending(fixture: &Fixture) -> Result<Ending> {
    let task = fixture
        .prepared
        .inputs
        .current_task
        .clone()
        .ok_or_else(|| error::config("fixture task missing"))?;
    let mut output = test_support::answer();
    output.task = Some(TaskReport {
        task: DecisionTaskVersion {
            task_id: task.task_id.to_string(),
            generation: task.generation,
        },
        disposition: TaskDisposition::Waiting,
        decision_call_id: Some(ID.to_owned()),
        checkpoint_id: None,
        covered_input_sequence: 7,
        effect_watermark: 5,
    });
    output.recovery = Some(TaskRecovery {
        task,
        history: Zeroizing::new(b"saved conversation".to_vec()),
    });
    Ok(Ending::Ran {
        output: Box::new(output),
        first_chunk: None,
    })
}

fn kept(fixture: &Fixture, sandbox: Box<dyn Sandbox>) -> Result<Kept> {
    Ok(Kept {
        recovery: Some(State {
            history: None,
            turn: None,
            base: None,
            base_paths: Vec::new(),
        }),
        sandbox,
        key: HoldKey {
            fleet: Uuid7::parse(&fixture.run.event.fleet_id)?,
            workspace: ID.to_owned(),
            limits: Limits::default(),
            policy: BuiltUnder::allowing(&[]),
        },
    })
}

#[tokio::test]
async fn a_durable_sandbox_that_cannot_save_its_history_is_killed_and_blocks_the_task() -> Result<()>
{
    for fault in ["interrupted", "busy executor", "refused freeze"] {
        let mut fixture = Fixture::new(|_| None)?;
        let root = tempfile::tempdir()?;
        let freezer = match fault {
            "busy executor" => Freezer::BusyExecutor,
            "refused freeze" => Freezer::RefusesFreeze,
            _ => Freezer::Works,
        };
        let (engine, sandbox) = sandbox(root.path(), freezer).await?;
        let mut ending = ending(&fixture)?;
        let kept = kept(&fixture, sandbox)?;
        let run = fixture.run()?;
        if fault == "interrupted" {
            run.interrupt.cancel();
        }

        let fate = run.keep(Some(kept), &mut ending).await;

        assert!(matches!(fate, Fate::Killed(_)), "{fault}: killed, not held");
        let Ending::Ran { output, .. } = &ending else {
            return Err(error::config("the run no longer ran"));
        };
        assert!(output.recovery.is_none(), "{fault}: history is released");
        let task = output
            .task
            .as_ref()
            .ok_or_else(|| error::config("task report missing"))?;
        assert_eq!(task.disposition, TaskDisposition::Blocked, "{fault}");
        assert_eq!(task.checkpoint_id, None, "{fault}");
        assert_eq!(
            engine.frozen.load(Ordering::SeqCst),
            0,
            "{fault}: nothing stays frozen"
        );
        assert_eq!(engine.thaw_attempts.load(Ordering::SeqCst), 0, "{fault}");
        drop(run);
        assert_eq!(fixture.rig.calls().len(), 0, "{fault}: nothing is saved");
    }
    Ok(())
}
