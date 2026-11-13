//! An approved setup grant restarts setup on its own (Indy, Nov 13, 2026):
//! the step the approval route runs, `Creations::resume_setup`, activates an
//! installing fleet whose last grant was just approved, and a denial leaves it
//! installing.

use super::super::{fixture::now, mint};
use super::fixture::Case;
use super::grants::{cards, requiring_github};
use af_crypto::entropy::Entropy;
use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};

async fn decided(c: &Case, decision: Decision) -> String {
    c.fund().await;
    let request = requiring_github(c).await;
    let saved = c.save(&mint(), request).await;
    assert_eq!(saved.status, "installing");
    let pending = cards(c, &saved.id).await;
    assert_eq!(pending.len(), 1, "one setup card: {pending:?}");
    let inbox = Inbox::new(
        c.base.db.clone(),
        c.bus.clone(),
        Admissions::new(c.base.db.clone(), c.bus.clone(), Entropy::new()),
    );
    let resolved = inbox
        .resolve(
            pending[0].0.as_str(),
            decision,
            c.base.owner.as_str(),
            "",
            Some(saved.id.as_str()),
        )
        .await
        .expect("an explicit decision");
    assert!(matches!(resolved, Resolution::Resolved(_)));
    if decision == Decision::Approved {
        c.creations
            .resume_setup(c.base.access(), &saved.id, now())
            .await
            .expect("setup resumes");
    }
    c.creations
        .read(c.base.access(), &saved.id)
        .await
        .expect("the fleet reads back")
        .status
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn approving_a_setup_grant_resumes_setup() {
    let approved = Case::create().await;
    assert_eq!(
        decided(&approved, Decision::Approved).await,
        "active",
        "the approval alone activates: no PATCH, no second Save"
    );
    approved.cleanup().await;

    let denied = Case::create().await;
    assert_eq!(
        decided(&denied, Decision::Denied).await,
        "installing",
        "a denial never activates"
    );
    denied.cleanup().await;
}
