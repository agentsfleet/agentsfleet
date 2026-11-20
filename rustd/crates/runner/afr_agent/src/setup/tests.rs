//! A setup run answers on the runner's own loop with `propose_setup` alone:
//! a valid proposal is kept on the base the run started from, a refused one
//! returns to the model as its call's result, and a corrected retry is the
//! one handed back.

use std::error::Error;

use af_wire::activity::ActivityFrame;
use guffbolt::Message;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::SetupRun;
use super::fixture::{
    ANSWER, ASKED, Context, HEAD, document, initialize, run_setup, setup_payload,
};
use crate::fixture::{Frames, Script, call, say, spent};
use crate::{Completed, Meter, ResultOutcome};

type TestResult = Result<(), Box<dyn Error>>;

/// The result text of the call the request `sent` answered last.
fn last_result(messages: &[Message]) -> Option<&str> {
    messages.iter().rev().find_map(|message| match message {
        Message::ToolResult { output, .. } => Some(output.as_str()),
        Message::User(_) | Message::Assistant { .. } => None,
    })
}

#[tokio::test]
async fn a_setup_run_keeps_a_valid_first_setup_and_answers() -> TestResult {
    let script = Script::new([
        vec![call("c1", "propose_setup", initialize("0 9 * * *"))],
        vec![say(ANSWER), spent(10, 0, 5)],
    ]);

    let run = setup_payload(&[], 0, None);
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    assert_eq!(output.result.outcome, ResultOutcome::Completed(Completed));
    assert_eq!(output.result.content, ANSWER);
    let proposal = output.proposal.ok_or("the run kept no proposal")?;
    assert_eq!(
        (proposal.base_candidate_id, proposal.base_revision),
        (None, 0)
    );
    assert!(
        proposal.definition.contains("0 9 * * *"),
        "{}",
        proposal.definition
    );
    Ok(())
}

#[tokio::test]
async fn a_refused_proposal_corrected_in_the_same_run_keeps_only_the_correction() -> TestResult {
    let script = Script::new([
        vec![call("c1", "propose_setup", initialize("61 * * * *"))],
        vec![call("c2", "propose_setup", initialize("0 9 * * *"))],
        vec![say(ANSWER)],
    ]);

    let run = setup_payload(&[], 0, None);
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    let sent = script.sent();
    let refused = sent
        .get(1)
        .and_then(|request| last_result(&request.messages))
        .ok_or("the correction's request holds no call result")?;
    assert!(refused.starts_with("[invalid_arguments]"), "{refused}");
    assert!(refused.contains("`schedule`"), "{refused}");
    let proposal = output
        .proposal
        .ok_or("the corrected proposal was not kept")?;
    assert!(
        proposal.definition.contains("0 9 * * *"),
        "{}",
        proposal.definition
    );
    assert!(
        !proposal.definition.contains("61 * * * *"),
        "{}",
        proposal.definition
    );
    assert_eq!(output.result.content, ANSWER);
    Ok(())
}

#[tokio::test]
async fn a_setup_run_whose_every_proposal_is_refused_hands_back_none() -> TestResult {
    let script = Script::new([
        vec![call("c1", "propose_setup", initialize("61 * * * *"))],
        vec![say("That schedule is not valid; which time do you want?")],
    ]);

    let run = setup_payload(&[], 0, None);
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    assert_eq!(output.proposal, None);
    assert_eq!(output.result.outcome, ResultOutcome::Completed(Completed));
    Ok(())
}

#[tokio::test]
async fn an_edit_is_kept_on_the_head_the_run_started_from() -> TestResult {
    let script = Script::new([
        vec![call(
            "c1",
            "propose_setup",
            json!({"proposal": {"kind": "edit", "base_candidate_id": HEAD,
                "base_candidate_revision": "2", "instruction_edits": [
                    {"kind": "append", "text": "\n\nEscalate anything urgent."}]}}),
        )],
        vec![say(ANSWER)],
    ]);
    let head = json!({"id": HEAD, "revision": 2, "definition": document("0 8 * * *")});

    let run = setup_payload(&[], 0, Some(&head));
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    let proposal = output.proposal.ok_or("the edit was not kept")?;
    assert_eq!(proposal.base_candidate_id.as_deref(), Some(HEAD));
    assert_eq!(proposal.base_revision, 2);
    let first = script.sent().into_iter().next().ok_or("no model call")?;
    let Some(Message::User(opening)) = first.messages.last() else {
        return Err("the first request does not end on the person's message".into());
    };
    assert!(
        opening.starts_with(super::context::CANDIDATE_LABEL),
        "{opening}"
    );
    assert!(opening.ends_with(ASKED), "{opening}");
    Ok(())
}

#[tokio::test]
async fn a_setup_run_offers_propose_setup_alone() -> TestResult {
    let script = Script::new([
        vec![call("c1", "shell", json!({"command": "ls"}))],
        vec![say(ANSWER)],
    ]);

    let run = setup_payload(&[], 0, None);
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    let sent = script.sent();
    for request in &sent {
        assert_eq!(request.tools, ["propose_setup"]);
    }
    let refused = sent
        .get(1)
        .and_then(|request| last_result(&request.messages))
        .ok_or("the shell call has no result")?;
    assert!(refused.starts_with("[tool_not_offered]"), "{refused}");
    assert_eq!(output.proposal, None);
    Ok(())
}

#[tokio::test]
async fn a_fleet_run_handed_to_setup_is_refused_before_any_model_call() -> TestResult {
    let script = Script::new([vec![say(ANSWER)]]);
    let mut run = setup_payload(&[], 0, None);
    run.owner = af_wire::run::Owner::Fleet {
        fleet_id: run.event.fleet_id.clone(),
    };

    let refused = run_setup(&script.replay(), &run, &Context::empty()).await;

    let Err(failure) = refused else {
        return Err("a fleet's run ran as a setup turn".into());
    };
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(script.sent().is_empty(), "the model was called");
    Ok(())
}

#[tokio::test]
async fn a_failed_model_call_fails_the_setup_run_with_no_answer() -> TestResult {
    let script = Script::failing(Vec::new(), || af_providers::Error::refused(503));

    let run = setup_payload(&[], 0, None);
    let output = run_setup(&script.replay(), &run, &Context::empty()).await?;

    assert!(
        matches!(output.result.outcome, ResultOutcome::Failed(_)),
        "{:?}",
        output.result.outcome
    );
    assert_eq!(output.result.content, "");
    assert_eq!(output.proposal, None);
    Ok(())
}

#[tokio::test]
async fn a_setup_run_streams_its_answer_live() -> TestResult {
    let script = Script::new([vec![say(ANSWER)]]);
    let frames = Frames::default();
    let sink = frames.sink();
    let (meter, stop, context) = (Meter::default(), CancellationToken::new(), Context::empty());
    let run = setup_payload(&[], 0, None);

    let setup = SetupRun {
        run: &run,
        context: &context,
        events: &sink,
        meter: &meter,
        stop: &stop,
    };
    super::run(&script.replay(), setup).await?;

    let streamed: String = frames
        .taken()
        .into_iter()
        .filter_map(|frame| match frame {
            ActivityFrame::FleetResponseChunk(chunk) => Some(chunk.text.into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(streamed, ANSWER);
    Ok(())
}
