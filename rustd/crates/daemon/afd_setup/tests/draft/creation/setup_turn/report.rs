//! A report finishes its message once: the answer, and the save the message
//! asked for, which installs the head the person reviewed or is kept on the
//! message as the refusal it met.

use std::sync::{Arc, Mutex, PoisonError};

use super::{
    ANSWER, Case, finished, funded, mint, name_draft, now, queued, report, reviewed, saved_fleet,
    send, take,
};
use af_core::{error_code, id::Uuid7};
use af_crypto::entropy::Entropy;
use af_fleet_runtime::Definition;
use afd_setup::{
    Creations, SavedFleet,
    types::{CreateDraft, Source, Start},
};
use afd_turns::Turns;
use serde_json::json;

/// The schedule the reviewed setup declares.
const SCHEDULE: &str = "0 9 * * *";

/// An upstream scheduler that records each schedule pushed to it.
#[derive(Debug, Default)]
struct Recording(Mutex<Vec<(String, String, String)>>);

impl Recording {
    fn pushed(&self) -> Vec<(String, String, String)> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl afd_cron::Scheduler for Recording {
    async fn upsert(
        &self,
        schedule: &Uuid7,
        cron: &str,
        timezone: &str,
        message: &str,
        _attempts: afd_cron::Attempts,
    ) -> afd_cron::Result<afd_cron::qstash::Registered> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push((
            cron.to_owned(),
            timezone.to_owned(),
            message.to_owned(),
        ));
        Ok(afd_cron::qstash::Registered {
            schedule_id: schedule.as_str().to_owned(),
        })
    }

    async fn remove(&self, _id: &str, _attempts: afd_cron::Attempts) -> afd_cron::Result<()> {
        Ok(())
    }

    async fn verify(
        &self,
        _signature: &str,
        _body: &[u8],
    ) -> afd_cron::Result<afd_cron::VerifiedFire> {
        // No fire reaches this scheduler; an alias is a refusal to hand back.
        Err(afd_cron::Recipe::parse("@daily", "UTC", "Daily check")
            .expect_err("an alias is refused"))
    }
}

/// A setup whose cron trigger runs in `timezone`.
fn scheduled(timezone: &str) -> Definition {
    serde_json::from_value(json!({
        "schema_version": 1,
        "description": "Explain failures.",
        "instructions_md": "Investigate failures.\n",
        "config": {"name": "checkout-watch", "x-agentsfleet": {
            "triggers": [{"type": "cron", "schedule": SCHEDULE, "timezone": timezone,
                          "message": "Daily check"}],
            "tools": [], "credentials": [], "network": {"allow": []},
            "gates": {"timeout_ms": 60000, "rules": []},
            "budget": {"daily_nanos": af_core::money::NANOS_PER_USD.to_string()}
        }}
    }))
    .expect("a scheduled setup passes the shared validator")
}

/// A reviewed draft of `definition` named `name`, its save message queued.
async fn saving(c: &Case, definition: Definition, name: &str) -> Uuid7 {
    let request = CreateDraft {
        source: Source::Definition {
            definition: Box::new(definition),
        },
        ..c.base.request(mint(), None)
    };
    let draft = c
        .base
        .drafts
        .create(c.base.access(), request, now())
        .await
        .expect("an idle draft")
        .draft;
    name_draft(c, &draft.id, &draft.etag, name).await;
    send(&c.base, &draft.id, "Save it", true).await;
    draft.id
}

/// The case's creations, pushing schedules to `scheduler`.
fn scheduling(c: &Case, scheduler: Arc<Recording>) -> Creations {
    let turns = Turns::new(c.base.db.clone(), c.bus.clone(), Entropy::new());
    Creations::new(c.setup.clone(), turns).with_scheduler(scheduler)
}

// Value: protects=Save in chat installs the reviewed head, activates it and
// registers its schedule once, and a replayed finish only re-checks the
// schedule; fails_when=the finish skips activation, re-activates a closed
// draft, or pushes the schedule twice; seam=the report half of
// afd_setup::Creations over live Postgres and NATS, with the upstream
// scheduler faked at its trait.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_chat_save_installs_the_head_and_registers_its_schedule_once() {
    let c = funded().await;
    let scheduler = Arc::new(Recording::default());
    let creations = scheduling(&c, Arc::clone(&scheduler));
    let draft = saving(&c, scheduled("UTC"), "support-triage").await;
    let turn = take(&c.base, &draft).await;
    assert!(turn.save_requested);
    report(&c, &creations, &turn)
        .await
        .expect("the save turn reports");

    let (fleet, status) = saved_fleet(&c.base, &draft).await.expect("a saved fleet");
    assert_eq!(status, "active");
    let (state, answer, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!(
        (state.as_str(), answer.as_deref(), refused),
        ("completed", Some(ANSWER), None)
    );
    let pushed = || {
        (
            SCHEDULE.to_owned(),
            "UTC".to_owned(),
            "Daily check".to_owned(),
        )
    };
    assert_eq!(scheduler.pushed(), [pushed()]);
    let read = creations
        .read(c.base.access(), &fleet)
        .await
        .expect("the saved fleet reads");
    assert_eq!(read.setup_revision, "1");

    // The report's copy replayed after the commit finds the draft closed: only
    // the schedule sweep is owed, and it has nothing left to push.
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the replay opens");
    let scope = creations
        .lock_setup_report(&mut tx, &draft, &turn.message_id)
        .await
        .expect("the replay locks")
        .expect("the draft is still there");
    tx.rollback().await.expect("the replay rolls back");
    creations
        .after_setup_report(&scope)
        .await
        .expect("the replayed finish");
    assert_eq!(scheduler.pushed(), [pushed()], "pushed once");
    let again = creations
        .read(c.base.access(), &fleet)
        .await
        .expect("the saved fleet reads");
    assert_eq!(
        (again.status.as_str(), again.setup_revision.as_str()),
        ("active", "1")
    );
    c.cleanup().await;
}

// Value: protects=a chat save pushes its schedules only once the fleet is
// active, so a fleet stuck installing fires nothing; fails_when=the finish
// registers the schedule of a fleet activation has not reached; seam=the
// report half of afd_setup::Creations over live Postgres and NATS, the wallet
// drained after the setup run started so activation stops at it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_chat_save_left_installing_registers_no_schedule() {
    let c = funded().await;
    let scheduler = Arc::new(Recording::default());
    let creations = scheduling(&c, Arc::clone(&scheduler));
    let draft = saving(&c, scheduled("UTC"), "installing-watch").await;
    let turn = take(&c.base, &draft).await;
    sqlx::query("UPDATE billing.wallets SET balance_nanos = 0 WHERE tenant_id = $1")
        .bind(&c.base.tenant)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("the wallet drains");

    report(&c, &creations, &turn)
        .await
        .expect("the save turn reports");

    let (_, status) = saved_fleet(&c.base, &draft).await.expect("a saved fleet");
    assert_eq!(status, "installing", "an empty wallet stops activation");
    assert!(
        scheduler.pushed().is_empty(),
        "an installing fleet fires nothing"
    );
    c.cleanup().await;
}

// Value: protects=a save the head cannot take is told to the person on the
// message, and installs nothing; fails_when=the refusal rolls back the answer,
// is reported as a report failure, or leaves a fleet; seam=the report half of
// afd_setup::Creations over live Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_save_the_head_cannot_take_is_kept_on_its_message() {
    let c = funded().await;
    let draft = reviewed(&c, "retired-policy").await;
    c.base.retire_head(&draft).await;
    send(&c.base, &draft, "Save it", true).await;
    let turn = take(&c.base, &draft).await;
    report(&c, &c.creations, &turn)
        .await
        .expect("a refused save still reports");

    let (state, answer, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!(state, "completed");
    assert_eq!(answer.as_deref(), Some(ANSWER), "the answer stands");
    assert_eq!(refused.as_deref(), Some(error_code::SETUP_POLICY.as_str()));
    assert_eq!(saved_fleet(&c.base, &draft).await, None);
    assert_eq!(c.fleet_count().await, 0, "nothing was installed");
    c.cleanup().await;
}

// Value: protects=a schedule the scheduler would not take refuses the save
// before any fleet exists; fails_when=the recipe check is skipped and a fleet
// with an unregistrable schedule installs; seam=the report half of
// afd_setup::Creations over live Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_save_whose_schedule_names_no_timezone_is_refused_on_its_message() {
    let c = funded().await;
    let draft = saving(&c, scheduled("Mars/Olympus_Mons"), "mars-watch").await;
    let turn = take(&c.base, &draft).await;
    report(&c, &c.creations, &turn)
        .await
        .expect("a refused save still reports");
    let (state, _, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!(state, "completed");
    assert_eq!(
        refused.as_deref(),
        Some(error_code::INVALID_REQUEST.as_str())
    );
    assert_eq!(c.fleet_count().await, 0, "nothing was installed");
    c.cleanup().await;
}

// Value: protects=a creator who lost the workspace gets no private write from
// a report: the message is cancelled, as their queued work is;
// fails_when=the report answers into a revoked creator's draft;
// seam=the report half of afd_setup::Creations over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_for_a_creator_who_lost_the_workspace_cancels_the_message() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.member_access(), "Watch checkout").await;
    let turn = take(&c.base, &draft).await;
    sqlx::query("DELETE FROM identity.memberships WHERE tenant_id=$1 AND user_id=$2")
        .bind(&c.base.tenant)
        .bind(&c.base.member)
        .execute(&mut *c.base.connection().await)
        .await
        .expect("revoke the member");
    report(&c, &c.creations, &turn)
        .await
        .expect("the report settles");
    let (state, answer, _) = finished(&c.base, &turn.message_id).await;
    assert_eq!((state.as_str(), answer), ("cancelled", None));
    c.cleanup().await;
}

// Value: protects=a report for a deleted draft finds nothing to finish, and a
// report's scope names its draft and none of the conversation it locked;
// fails_when=the lock answers a scope for a gone draft, or Debug prints the
// creator; seam=Creations::lock_setup_report over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_report_scope_names_only_its_draft_and_a_deleted_draft_has_none() {
    let c = funded().await;
    let draft = queued(&c.base, c.base.access(), "Watch checkout").await;
    let turn = take(&c.base, &draft).await;
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the report opens");
    let scope = c
        .creations
        .lock_setup_report(&mut tx, &draft, &turn.message_id)
        .await
        .expect("the report locks")
        .expect("the draft is there");
    let shown = format!("{scope:?}");
    assert!(
        shown.starts_with("SetupReportScope { draft_id: "),
        "{shown}"
    );
    assert!(shown.contains(draft.as_str()), "{shown}");
    assert!(shown.ends_with(", .. }"), "{shown}");
    assert!(!shown.contains(c.base.owner.as_str()), "{shown}");
    tx.rollback().await.expect("the report rolls back");

    c.base
        .drafts
        .delete(c.base.access(), &draft)
        .await
        .expect("the creator deletes the draft");
    let mut tx = sqlx::Transaction::begin(c.base.connection().await, None)
        .await
        .expect("the report opens");
    let gone = c
        .creations
        .lock_setup_report(&mut tx, &draft, &turn.message_id)
        .await
        .expect("the report locks");
    assert!(gone.is_none(), "a deleted draft has nothing to finish");
    tx.rollback().await.expect("the report rolls back");
    c.cleanup().await;
}

/// A saved, active fleet named `name`, and an improvement chat on its head
/// whose save message is queued.
async fn improving(c: &Case, name: &str) -> (SavedFleet, Uuid7) {
    let (_, request) = c.request(name, Start::None).await;
    let saved = c.save(&mint(), request).await;
    assert_eq!(saved.status, "active");
    let mut request = c.base.request(mint(), None);
    request.source = Source::Fleet {
        fleet_id: saved.id.clone(),
        setup_revision: saved.setup_revision.clone(),
    };
    let chat = c
        .base
        .drafts
        .create(c.base.access(), request, now())
        .await
        .expect("an improvement chat on the saved head")
        .draft
        .id;
    send(&c.base, &chat, "Save it", true).await;
    (saved, chat)
}

// Value: protects=Save in an improvement chat lands the head as the fleet's
// next revision and keeps the fleet running it; fails_when=a chat pinned to a
// fleet installs a second fleet, or the revision is not activated;
// seam=the report half of afd_setup::Creations over live Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_chat_save_on_an_improvement_lands_the_fleets_next_revision() {
    let c = funded().await;
    let (saved, chat) = improving(&c, "improve-in-chat").await;
    let turn = take(&c.base, &chat).await;
    report(&c, &c.creations, &turn)
        .await
        .expect("the save reports");
    let (_, _, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!(refused, None);
    let read = c
        .creations
        .read(c.base.access(), &saved.id)
        .await
        .expect("the improved fleet reads");
    assert_eq!(
        (read.status.as_str(), read.setup_revision.as_str()),
        ("active", "2")
    );
    assert_eq!(c.fleet_count().await, 1, "the chat saved onto its fleet");
    c.cleanup().await;
}

// Value: protects=an improvement saved in chat after its fleet was archived is
// refused on the message and moves no revision; fails_when=the archive check is
// dropped and an archived fleet takes a revision; seam=the report half of
// afd_setup::Creations over live Postgres and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_chat_save_on_an_archived_fleet_is_refused_on_its_message() {
    let c = funded().await;
    let (saved, chat) = improving(&c, "improve-archived-chat").await;
    let turn = take(&c.base, &chat).await;
    c.creations
        .patch(
            c.base.access(),
            &saved.id,
            Some(&saved.etag),
            Some("archived"),
            None,
            now(),
        )
        .await
        .expect("Archive");
    report(&c, &c.creations, &turn)
        .await
        .expect("a refused save still reports");
    let (state, _, refused) = finished(&c.base, &turn.message_id).await;
    assert_eq!(state, "completed");
    assert_eq!(refused.as_deref(), Some(error_code::SETUP_STATE.as_str()));
    let read = c
        .creations
        .read(c.base.access(), &saved.id)
        .await
        .expect("the archived fleet reads");
    assert_eq!(
        (read.status.as_str(), read.setup_revision.as_str()),
        ("archived", "1")
    );
    c.cleanup().await;
}
