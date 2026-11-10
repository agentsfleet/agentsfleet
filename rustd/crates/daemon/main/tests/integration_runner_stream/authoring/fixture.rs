//! A production daemon and native runner share one authoring lease.

use af_core::{clock, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{authoring as wire, lease::TaggedLease, protocol::Down};
use afd_authoring::{
    Access, Drafts,
    types::{CreateDraft, Source},
};
use afd_db::test_util::{OwnedRows, mint_id};
use afd_runner::{Runners, capacity::authority::Decision};
use afr_supervisor::{config::Config, test_util::Session};
use agentsfleetd::{serve::Booted, supervisor::Supervisor};
use std::sync::Arc;

use super::{DEADLINE, decision::Boundary, provider::Provider, seed::Rows};

mod connection;

/// Test-only owner of a real daemon, native session, provider HTTP boundary and seeded rows.
pub(super) struct AuthoringFixture {
    pub(super) booted: Booted,
    pub(super) rows: Rows,
    pub(super) runner: Uuid7,
    pub(super) token: String,
    pub(super) session: Session,
    pub(super) config: Config,
    pub(super) lease: wire::Lease<serde_json::Value>,
    pub(super) drafts: Drafts,
    pub(super) provider: Provider,
    cleanup: OwnedRows,
    decision: Option<afd_admin::test_util::DecisionModelFixture>,
    _exclusive: tokio::sync::MutexGuard<'static, ()>,
}

impl AuthoringFixture {
    pub(super) async fn start(supervisor: &mut Supervisor, blocked: bool) -> Self {
        Self::configured(supervisor, blocked, None).await
    }

    pub(super) async fn with_decision(supervisor: &mut Supervisor, boundary: &Boundary) -> Self {
        Self::configured(supervisor, true, Some(boundary)).await
    }

    async fn configured(
        supervisor: &mut Supervisor,
        blocked: bool,
        boundary: Option<&Boundary>,
    ) -> Self {
        crate::support::install_subscriber();
        let exclusive = crate::e2e::SCENARIOS.lock().await;
        let (booted, provider) = connection::boot(supervisor, blocked, boundary).await;
        let decision = match boundary {
            Some(boundary) => Some(boundary.model(&booted.database).await),
            None => None,
        };
        let workspace = boundary.map(|boundary| boundary.workspace.clone());
        let rows = Rows::seed(&booted.database, workspace).await;
        let (runner, token, config, session) = connection::connect(&booted).await;
        if let (Some(boundary), Some(model)) = (boundary, decision.as_ref()) {
            boundary
                .assign(&booted.database, &runner, &model.model.id)
                .await;
        }
        let drafts = Drafts::new(
            booted.database.clone(),
            Entropy::new(),
            Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
                booted.database.clone(),
            )),
        );
        let lease = queue(&drafts, &rows, &session).await;
        let cleanup = rows.owned_rows(&runner);
        Self {
            booted,
            rows,
            runner,
            token,
            session,
            config,
            lease,
            drafts,
            provider,
            cleanup,
            decision,
            _exclusive: exclusive,
        }
    }

    pub(super) fn id(&self) -> Uuid7 {
        Uuid7::parse(&self.lease.lease_id).expect("native lease id")
    }

    pub(super) fn access(&self) -> Access<'_> {
        Access {
            workspace: &self.rows.workspace,
            subject: self.rows.user.as_str(),
        }
    }

    pub(super) fn model(&self) -> wire::ModelCall {
        wire::ModelCall {
            capability_token: self.lease.model_access.token.clone(),
            attempt_id: self.lease.attempt_id.clone(),
            fence: self.lease.fence.clone(),
            dispatch_id: mint_id().to_string(),
            call_index: 1,
            request: wire::ModelRequest {
                messages: vec![wire::ModelMessage::User {
                    text: "Review this setup".into(),
                }],
                max_output_tokens: 64,
                supports_compaction: true,
            },
        }
    }

    pub(super) fn proposal(&self) -> wire::ActivityRequest {
        let mut config: serde_json::Value =
            serde_json::from_str(crate::e2e_seed::FLEET_CONFIG_JSON).expect("fixture setup config");
        config
            .as_object_mut()
            .expect("setup config object")
            .insert("name".into(), serde_json::json!("support-triage"));
        let definition = crate::e2e_native::definition(
            &serde_json::to_string(&config).expect("purpose-named setup config"),
            "Review incoming support requests.",
        );
        wire::ActivityRequest {
            fence: self.lease.fence.clone(),
            sequence: self.lease.sequence.clone(),
            activity_sequence: "1".into(),
            event: wire::ActivityEvent::Proposal {
                proposal: wire::Proposal::Initialize {
                    definition: Box::new(
                        serde_json::to_value(definition).expect("native definition"),
                    ),
                },
            },
        }
    }

    pub(super) fn report(&self, calls: u32) -> wire::Report {
        wire::Report::Terminal {
            accounting: wire::Accounting {
                fence: self.lease.fence.clone(),
                report_id: mint_id().to_string(),
                outcome: wire::Outcome::Completed,
                failure_reason: None,
                usage: meter(calls),
            },
            answer: Some(super::ANSWER.into()),
        }
    }

    pub(super) fn renewal(&self, calls: u32) -> wire::Renew {
        wire::Renew {
            fence: self.lease.fence.clone(),
            usage: meter(calls),
        }
    }

    pub(super) async fn replace_authority(&self) {
        let presented = afd_auth::credential::Presented::new(&self.token).expect("runner bearer");
        let digest = afd_auth::directory::Digest::of(&presented);
        let decision = Runners::new(self.booted.database.clone(), Entropy::new())
            .acquire_connection(&self.runner, digest, None, clock::now())
            .await
            .expect("replacement authority");
        assert!(matches!(decision, Decision::Allowed(_)));
    }

    pub(super) async fn finish(mut self, supervisor: Supervisor) {
        if let Err(error) = self.session.close().await {
            assert!(
                [
                    af_core::error_code::RUN_STREAM_SUPERSEDED,
                    af_core::error_code::RUN_INVALID_RUNNER_TOKEN,
                    af_core::error_code::RUN_ADMIN_STATE_BLOCKED,
                ]
                .contains(&error.code()),
                "unexpected native owner failure: {error}"
            );
        }
        supervisor.shutdown().await;
        self.provider.close().await;
        if let Some(model) = self.decision.take() {
            super::decision::finish_model(&self.booted.database, &self.runner, model).await;
        }
        self.cleanup
            .finish()
            .await
            .expect("remove owned authoring rows");
    }
}

pub(super) fn meter(calls: u32) -> wire::Meter {
    wire::Meter {
        knowledge: wire::Knowledge::Known,
        calls,
        input_tokens: None,
        output_tokens: None,
        provider_cost_nanos: None,
        runtime_ms: 0,
    }
}

async fn queue(drafts: &Drafts, rows: &Rows, session: &Session) -> wire::Lease<serde_json::Value> {
    let access = Access {
        workspace: &rows.workspace,
        subject: rows.user.as_str(),
    };
    let request = CreateDraft {
        save: false,
        source: Source::Blank,
        authoring_model_ref: rows.model.to_string(),
        message: Some("Review the proposed setup".into()),
        operation_id: mint_id(),
    };
    let created = drafts
        .create(access, request, clock::now())
        .await
        .expect("queue authoring message");
    let frame = tokio::time::timeout(DEADLINE, session.offer())
        .await
        .expect("authoring offer deadline")
        .expect("authoring offer");
    let message: Down<'_> = frame.decode().expect("native offer decode");
    let lease = match message {
        Down::Offer {
            lease: TaggedLease::Authoring(lease),
        } => Some(*lease),
        _ => None,
    }
    .expect("expected authoring offer");
    assert_eq!(lease.draft_id, created.draft.id.as_str());
    lease
}
