//! A retained activation accepts only its own confirmed admission, and retries
//! preserve its fleet and event while confirmation is unavailable.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use afd_admission::Admissions;
use afd_authoring::{
    Creations, SavedFleet,
    types::{SaveFleet, Start, StartOutcome, StartState},
};
use afd_core::{id::Uuid7, test_util::trace::Capture};
use afd_crypto::entropy::Entropy;

struct Retained {
    case: Case,
    key: Uuid7,
    request: SaveFleet,
    saved: SavedFleet,
    event: String,
}

impl Retained {
    async fn create(name: &str) -> Self {
        let case = Case::create().await;
        let (_, request) = case
            .request(
                name,
                Start::RunNow {
                    message: "Run once".to_owned(),
                },
            )
            .await;
        case.fund().await;
        let key = mint();
        let saved = case.save(&key, request.clone()).await;
        let event = match &saved.start {
            StartOutcome::RunNow {
                state: StartState::Accepted,
                event_id: Some(event),
                ..
            } => Some(event.clone()),
            _ => None,
        }
        .expect("the funded activation admits its initial run");
        Self {
            case,
            key,
            request,
            saved,
            event,
        }
    }

    async fn pending(&self, expected: &str) {
        sqlx::query("UPDATE fleet.fleets SET initial_start=jsonb_set(initial_start, '{outcome}', $2::jsonb) WHERE id=$1::uuid")
            .bind(self.saved.id.as_str())
            .bind(serde_json::json!({"kind":"run_now", "state":"pending", "event_id":expected}).to_string())
            .execute(&mut *self.case.base.connection().await).await.expect("unconfirmed activation");
    }

    async fn retry(&self, creations: &Creations, accepted: bool, expected: &str) {
        let saved = creations
            .save(
                self.case.base.access(),
                &self.key,
                self.request.clone(),
                now(),
            )
            .await
            .expect("retained Save");
        assert_eq!(saved.id, self.saved.id);
        let (state, event_id, error_code) = match saved.start {
            StartOutcome::RunNow {
                state,
                event_id,
                error_code,
            } => Some((state, event_id, error_code)),
            StartOutcome::None => None,
        }
        .expect("the initial run is retained");
        assert_eq!(matches!(state, StartState::Accepted), accepted);
        assert!(matches!(state, StartState::Pending) || accepted);
        assert_eq!(event_id.as_deref(), Some(expected));
        assert!(error_code.is_none());
        assert_eq!(self.case.fleet_count().await, 1);
        assert_eq!(
            self.case.admissions(&saved.id).await.as_slice(),
            std::slice::from_ref(&self.event)
        );
    }
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_missing_confirmation_keeps_the_saved_identity_until_the_same_key_answers() {
    let retained = Retained::create("confirm-missing").await;
    retained.pending(&retained.event).await;
    let mut connection = retained.case.base.connection().await;
    let key: String =
        sqlx::query_scalar("SELECT producer_key FROM fleet.admissions WHERE id=$1::uuid")
            .bind(&retained.event)
            .fetch_one(&mut *connection)
            .await
            .expect("initial producer key");
    sqlx::query("UPDATE fleet.admissions SET producer_key=$2 WHERE id=$1::uuid")
        .bind(&retained.event)
        .bind(mint().as_str())
        .execute(&mut *connection)
        .await
        .expect("hide confirmation");
    retained
        .retry(&retained.case.creations, false, &retained.event)
        .await;
    sqlx::query("UPDATE fleet.admissions SET producer_key=$2 WHERE id=$1::uuid")
        .bind(&retained.event)
        .bind(key)
        .execute(&mut *connection)
        .await
        .expect("restore confirmation");
    drop(connection);
    retained
        .retry(&retained.case.creations, true, &retained.event)
        .await;
    retained.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_confirmation_for_another_event_cannot_accept_the_initial_run() {
    let retained = Retained::create("confirm-mismatch").await;
    let unexpected = mint();
    retained.pending(unexpected.as_str()).await;
    retained
        .retry(&retained.case.creations, false, unexpected.as_str())
        .await;
    retained.pending(&retained.event).await;
    retained
        .retry(&retained.case.creations, true, &retained.event)
        .await;
    retained.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_confirmation_outage_is_logged_and_a_retry_accepts_without_another_admission() {
    let retained = Retained::create("confirm-outage").await;
    retained.pending(&retained.event).await;
    let admissions = Admissions::new(
        afd_db::test_util::unreachable_db(),
        retained.case.queue.clone(),
        Entropy::new(),
    );
    let creations = Creations::new(retained.case.setup.clone(), admissions);
    let log = Capture::install();
    retained.retry(&creations, false, &retained.event).await;
    let fields = log.only("fleet_initial_run_unconfirmed").fields;
    assert_eq!(
        fields.get("fleet_id").map(String::as_str),
        Some(retained.saved.id.as_str())
    );
    assert_eq!(
        fields.get("error_code").map(String::as_str),
        Some(afd_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str())
    );
    drop(log);
    retained
        .retry(&retained.case.creations, true, &retained.event)
        .await;
    retained.case.cleanup().await;
}
