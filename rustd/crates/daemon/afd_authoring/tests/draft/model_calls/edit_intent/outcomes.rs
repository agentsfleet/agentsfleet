//! An unsolicited model proposal cannot turn a question into a setup mutation.

use super::super::{fixture, text_edits::native};
use super::{
    fixture::{Case, SOAK, reply},
    server::Server,
};
use af_core::error_code;
use af_wire::authoring::{ActivityEvent, ProposalCall, ProposalRejection};

// Value: protects=the soak question leaves the candidate unchanged even if the setup model proposes an edit;
// fails_when=the newest-message decision is skipped, the refusal remains executable, or activity bypasses it;
// seam=real database/vault/biller plus physical chat and TLS decision fixtures; no_claim=live model grading.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_question_preserves_the_candidate_even_when_the_setup_model_proposes_an_edit() {
    let mut server = Server::start(reply(0.01)).await;
    let case = Case::create(&server, SOAK).await;
    let request = fixture::request(&case.attempt);
    let response = case
        .calls
        .call(&case.f.runner, &case.attempt.lease, request.clone());
    let observe = async {
        let body = server.received().await;
        assert!(
            body.to_string().contains(SOAK),
            "use accepted message, not runner context"
        );
        assert!(!body.to_string().contains("Investigate the checkout"));
        assert_eq!(
            body.get("questions")
                .expect("questions field")
                .as_object()
                .expect("named questions")
                .len(),
            1
        );
        assert!(body.to_string().contains("Does this message ask to change the fleet's setup (instructions, configuration, name, schedule or access)?"));
        server.finish().await;
    };
    let (response, ()) = tokio::join!(response, observe);
    let response = response.expect("question answered");
    assert!(matches!(
        response.proposal_calls.as_slice(),
        [ProposalCall::Refused {
            reason: ProposalRejection::EditNotRequested,
            ..
        }]
    ));
    assert!(response.text.split_whitespace().count() <= 30);
    let stored = case
        .f
        .drafts
        .read(case.f.access(), &case.attempt.draft.id)
        .await
        .expect("candidate");
    assert_eq!(
        serde_json::to_value(&stored.candidate).expect("stored candidate JSON"),
        serde_json::to_value(&case.attempt.draft.candidate).expect("original candidate JSON")
    );
    let forged = edit(&case);
    let fault = case
        .f
        .drafts
        .activity(&case.f.runner, &case.attempt.lease, forged)
        .await
        .expect_err("native veto survives runner bypass");
    assert_eq!(fault.code(), error_code::INVALID_REQUEST);
    let replay = case
        .calls
        .call(&case.f.runner, &case.attempt.lease, request)
        .await
        .expect("saved refusal");
    assert!(replay.replayed);
    assert!(matches!(
        replay.proposal_calls.as_slice(),
        [ProposalCall::Refused {
            reason: ProposalRejection::EditNotRequested,
            ..
        }]
    ));
    assert_eq!(case.provider.count(), 1);
    assert_eq!(case.journal().await, ("no".into(), Some(112), true));
    assert_charge(&case).await;
    case.finish().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_explicit_setup_change_remains_an_ordinary_validated_edit() {
    let mut server = Server::start(reply(0.99)).await;
    let case = Case::create(&server, "Add an instruction to prefer clarity.").await;
    let response = case.calls.call(
        &case.f.runner,
        &case.attempt.lease,
        fixture::request(&case.attempt),
    );
    let observe = async {
        server.received().await;
        server.finish().await;
    };
    let (response, ()) = tokio::join!(response, observe);
    let response = response.expect("requested edit");
    let mut activity = case.attempt.initialize(1);
    activity.event = ActivityEvent::Proposal {
        proposal: native(&response).clone(),
    };
    let accepted = case
        .f
        .drafts
        .activity(&case.f.runner, &case.attempt.lease, activity)
        .await
        .expect("native edit");
    assert_eq!(accepted.candidate.expect("changed candidate").revision, "2");
    let stored = case
        .f
        .drafts
        .read(case.f.access(), &case.attempt.draft.id)
        .await
        .expect("stored edit");
    assert_eq!(
        stored
            .candidate
            .expect("candidate")
            .definition
            .instructions_md(),
        "Investigate failures.\n\nPrefer clarity."
    );
    assert_eq!(case.journal().await, ("yes".into(), Some(112), true));
    assert_charge(&case).await;
    case.finish().await;
}

pub(super) fn edit(case: &Case) -> afd_authoring::ActivityInput {
    let base = case.attempt.draft.candidate.as_ref().expect("base");
    let mut activity = case.attempt.initialize(1);
    activity.event = ActivityEvent::Proposal {
        proposal: af_wire::authoring::Proposal::Edit {
            base_candidate_id: base.id.to_string(),
            base_candidate_revision: base.revision.clone(),
            config_ops: Vec::new(),
            instruction_edits: vec![af_fleet_runtime::definition::InstructionEdit {
                start_byte: base.definition.instructions_md().len(),
                end_byte: base.definition.instructions_md().len(),
                text: "\nPrefer clarity.".into(),
            }],
        },
    };
    activity
}

async fn assert_charge(case: &Case) {
    let charge: (String, String, i64, i64) = sqlx::query_as("SELECT work_type, provider, input_token_count, output_token_count FROM billing.usage_lines WHERE work_id=$1 AND kind = $2")
        .bind(&case.attempt.id).bind(af_wire::usage::LineKind::Decision.as_str())
        .fetch_one(&mut *case.f.connection().await).await.expect("setup usage line");
    assert_eq!(
        charge,
        ("authoring_attempt".into(), "typesafe".into(), 100, 3)
    );
    assert!(
        !settled(case).await,
        "decision recording never debits inline"
    );
    case.f.relay(case.attempt.id.as_str()).await;
    assert!(
        settled(case).await,
        "the shared relay settles decision usage"
    );
    case.f.relay(case.attempt.id.as_str()).await;
    assert!(
        settled(case).await,
        "repeated relay leaves the receipt settled"
    );
}

async fn settled(case: &Case) -> bool {
    sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM billing.usage_lines WHERE work_id=$1 AND kind = $2",
    )
    .bind(&case.attempt.id)
    .bind(af_wire::usage::LineKind::Decision.as_str())
    .fetch_one(&mut *case.f.connection().await)
    .await
    .expect("native decision settlement receipt")
}
