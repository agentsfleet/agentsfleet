//! A setup draft's run builds no sandbox, reads its draft's context through
//! the fenced verb, and reports the proposal its engine kept, or none when
//! the engine could not run it.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use af_wire::policy::ExecutionPolicy;
use af_wire::run::{Owner, RunPayload, SetupProposal, SetupWork};
use afr_agent::{
    AgentEngine, AgentRun, Completed, ExecutionResult, Needs, ResultOutcome, RunOutput,
    SetupOutput, SetupRun,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::client::Verb;
use crate::test_support::{
    FENCING, FLEET_ID, FakeEngine, OUTCOME, PROCESSED, RUN_ID, Rig, STARTUP_POSTURE, bounded,
    daemon, payload, reported,
};

/// What the scripted setup answers.
const ANSWER: &str = "A daily check at nine, UTC.";
/// The head candidate the kept proposal names.
const HEAD: &str = "01890a5d-ac96-774b-bcce-b302099a80c1";
/// The kept setup, as the report carries it.
const DEFINITION: &str = r#"{"schema_version":1}"#;

/// An engine whose setup reads its context, then hands back `kept`, or
/// fails when `kept` is `None`. It runs no fleet turn.
#[derive(Debug)]
struct Setup {
    kept: Option<SetupOutput>,
    /// Whether each load answered.
    loads: Arc<Mutex<Vec<bool>>>,
}

#[async_trait::async_trait]
impl AgentEngine for Setup {
    /// Claims a sandbox: a setup run builds none, whatever its engine says.
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> afr_agent::Result<Needs> {
        Ok(Needs { sandbox: true })
    }

    async fn run(&self, _run: AgentRun<'_>) -> afr_agent::Result<RunOutput> {
        Err(afr_agent::Error::setup_context(crate::error::config(
            "a setup engine runs no fleet turn",
        )))
    }

    async fn setup(&self, run: SetupRun<'_>) -> afr_agent::Result<SetupOutput> {
        let loaded = run.context.load().await.is_ok();
        self.loads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(loaded);
        self.kept.clone().ok_or_else(|| {
            afr_agent::Error::setup_context(crate::error::config("the model is unreachable"))
        })
    }
}

/// The setup a run answered with, keeping a proposal on revision 2 of the head.
fn answered() -> SetupOutput {
    SetupOutput {
        result: ExecutionResult {
            outcome: ResultOutcome::Completed(Completed),
            content: ANSWER.into(),
            token_count: 15,
            wall_seconds: 1,
            input_tokens: 10,
            cached_input_tokens: 0,
            output_tokens: 5,
        },
        proposal: Some(SetupProposal {
            base_candidate_id: Some(Cow::Borrowed(HEAD)),
            base_revision: 2,
            definition: Cow::Borrowed(DEFINITION),
        }),
    }
}

/// The fixture run, owned by a draft and carrying its setup part.
fn draft_run() -> RunPayload<'static> {
    let mut run = payload(RUN_ID, FLEET_ID);
    run.owner = Some(Owner::Draft {
        draft_id: FLEET_ID.into(),
    });
    run.setup = Some(SetupWork {
        sequence: 2,
        candidate: None,
        save_requested: false,
        tool: json!({}),
        history_sequences: vec![1],
    });
    run
}

/// A rig over the healthy daemon whose engine is `engine`.
fn rig(engine: Setup) -> Rig {
    Rig::running(daemon(|_call| None), FakeEngine::default(), engine)
}

#[tokio::test(start_paused = true)]
async fn a_setup_run_builds_no_sandbox_and_reports_its_kept_proposal() {
    let loads = Arc::default();
    let mut rig = rig(Setup {
        kept: Some(answered()),
        loads: Arc::clone(&loads),
    });

    bounded(
        "Lessee::setup",
        rig.lessee.setup(&draft_run(), CancellationToken::new()),
    )
    .await
    .unwrap();

    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "a sandbox was prepared"
    );
    let calls = rig.calls();
    let report = reported(&calls);
    assert_eq!(report[OUTCOME], PROCESSED);
    assert_eq!(report["response_text"], ANSWER);
    assert_eq!(report["setup"]["proposal"]["base_candidate_id"], HEAD);
    assert_eq!(report["setup"]["proposal"]["base_revision"], 2);
    assert_eq!(report["setup"]["proposal"]["definition"], DEFINITION);
    let load = calls
        .iter()
        .find(|call| call.verb == Verb::ContextLoad)
        .and_then(|call| call.body.as_ref())
        .map(|body| serde_json::from_slice::<serde_json::Value>(body).unwrap());
    assert_eq!(load, Some(json!({"fencing_token": FENCING})));
    assert_eq!(*loads.lock().unwrap(), [true]);
}

#[tokio::test(start_paused = true)]
async fn a_setup_run_its_engine_could_not_run_reports_no_proposal() {
    let mut rig = rig(Setup {
        kept: None,
        loads: Arc::default(),
    });

    bounded(
        "Lessee::setup",
        rig.lessee.setup(&draft_run(), CancellationToken::new()),
    )
    .await
    .unwrap();

    assert_eq!(
        rig.prepared.load(Ordering::SeqCst),
        0,
        "a sandbox was prepared"
    );
    let report = reported(&rig.calls());
    assert_eq!(report[OUTCOME], "fleet_error");
    assert_eq!(report["failure_reason"], STARTUP_POSTURE);
    assert_eq!(report["setup"], json!({}), "a failed run kept no proposal");
}

#[tokio::test(start_paused = true)]
async fn a_fleet_run_never_reaches_the_setup_engine() {
    let loads = Arc::default();
    let mut rig = rig(Setup {
        kept: Some(answered()),
        loads: Arc::clone(&loads),
    });

    // A fleet's run has no owner part, so it takes the fleet path.
    rig.run(&payload(RUN_ID, FLEET_ID)).await.unwrap();

    assert!(
        loads.lock().unwrap().is_empty(),
        "a fleet run read setup context"
    );
    assert!(
        rig.calls()
            .iter()
            .all(|call| call.verb != Verb::ContextLoad),
        "a fleet run spoke the context verb"
    );
}
