//! A production daemon and native runner share one authoring lease.

use af_core::{clock, env::MapEnv, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_wire::{authoring as wire, lease::TaggedLease, protocol::Down};
use afd_authoring::{
    Access, Drafts,
    types::{CreateDraft, Source},
};
use afd_db::test_util::{OwnedRows, mint_id};
use afd_runner::{Runners, capacity::authority::Decision};
use afr_supervisor::{
    config::{self, Config},
    test_util::Session,
};
use agentsfleetd::{
    serve::{Booted, boot_with_authoring_boundary},
    supervisor::Supervisor,
};
use std::sync::Arc;

use super::{DEADLINE, provider::Provider, seed::Rows};

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
    _exclusive: tokio::sync::MutexGuard<'static, ()>,
}

impl AuthoringFixture {
    pub(super) async fn start(supervisor: &mut Supervisor, blocked: bool) -> Self {
        crate::support::install_subscriber();
        let exclusive = crate::e2e::SCENARIOS.lock().await;
        let (booted, provider) = boot(supervisor, blocked).await;
        let rows = Rows::seed(&booted.database).await;
        let runners = Runners::new(booted.database.clone(), Entropy::new());
        let registered = runners
            .register(&crate::e2e_seed::enrolment(), clock::now())
            .await
            .expect("register native runner");
        let token = registered.token.expose().to_owned();
        let config = native_config(&booted, &token);
        let session = Session::connect(&config).await.expect("native connection");
        session.capacity(1);
        let beat_bytes = serde_json::to_vec(&crate::wire::capable_beat()).expect("capability JSON");
        let beat = serde_json::from_slice(&beat_bytes).expect("capability report");
        session.heartbeat(&beat).await.expect("native heartbeat");
        let drafts = Drafts::new(
            booted.database.clone(),
            Entropy::new(),
            Arc::new(afd_billing::ledger::LedgerBiller::on_system_clock(
                booted.database.clone(),
            )),
        );
        let lease = queue(&drafts, &rows, &session).await;
        let cleanup = rows.owned_rows(&registered.runner_id);
        Self {
            booted,
            rows,
            runner: registered.runner_id,
            token,
            session,
            config,
            lease,
            drafts,
            provider,
            cleanup,
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
            .insert("name".into(), serde_json::json!("Support triage"));
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

fn native_config(booted: &Booted, token: &str) -> Config {
    let api = format!("http://{}", booted.address);
    let origin = format!("https://localhost:{}", booted.runner_stream_address.port());
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    Config::from_env(&MapEnv::from_pairs([
        (config::ENV_API_URL, api.as_str()),
        (config::ENV_RUNNER_TOKEN, token),
        (config::ENV_STREAM_URL, origin.as_str()),
        (config::ENV_STREAM_CA, root.as_str()),
    ]))
    .expect("native runner configuration")
}

async fn boot(supervisor: &mut Supervisor, blocked: bool) -> (Booted, Provider) {
    let provider = Provider::open(blocked).await;
    let transport = afd_authoring::Network::new()
        .expect("guarded client")
        .with_test_endpoint(provider.endpoint.clone())
        .expect("provider boundary");
    let env = crate::e2e::daemon_environment(
        &crate::e2e::lane(crate::e2e::DATABASE_LANE_KNOB),
        None,
        &[],
    );
    let booted = boot_with_authoring_boundary(&env, 0, supervisor, transport)
        .await
        .expect("daemon boot");
    (booted, provider)
}
