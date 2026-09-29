//! Repair-verification dispatch when the admission or the claim does not go
//! the way the pass expected.
//!
//! Each failure is injected inside Postgres by a trigger scoped to this test's
//! verifier fleet, so it fires at the exact point in the dispatch it names and
//! nowhere else; the trigger is dropped when the test ends.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_crypto::entropy::Entropy;
use afd_runner::sweep::Sweep as _;
use afd_runner::sweep::repair::Repairs;
use afd_wire::event::EventType;
use sqlx::{AssertSqlSafe, Row as _};

use crate::integration_repair_dispatch::{Fixture, REPAIR_LANE};
use crate::support::{Recorder, connect_redis};

/// The claim token the stand-in replica takes the intent under.
const OTHER_REPLICA: &str = "0195b4ba-8d3a-7000-8abc-0000000e0001";

/// The event a failed dispatch is logged under.
const EVENT_DISPATCH_FAILED: &str = "repair_verification_dispatch_failed";

/// The pre-admission's body. Its digest need not match the pass's: the key is
/// the identity, and the first payload stands.
const EARLIER_BODY: &str = r#"{"earlier":"dispatch"}"#;

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_dispatch_whose_event_already_exists_completes_on_that_event() {
    let _lane = REPAIR_LANE.lock().await;
    let fixture = Fixture::create().await;
    fixture.seed_intent().await;
    let admissions = Admissions::for_tests(fixture.database.clone(), connect_redis().await);
    // An earlier pass admitted the event and lapsed before it could record it.
    let earlier = admissions
        .admit(Admission {
            producer: Producer::RepairVerification,
            key: Key::Repeated(&fixture.verification),
            fleet: &fixture.verifier_fleet,
            workspace: &fixture.workspace,
            actor: "repair-verifier",
            event_type: EventType::Chat,
            request_json: EARLIER_BODY,
            reply: Reply::None,
        })
        .await
        .expect("the earlier pass's admission lands");

    Repairs::new(fixture.database.clone(), admissions, Entropy::new())
        .sweep()
        .await
        .expect("the pass runs");

    let recorded = fixture.verification().await;
    assert_eq!(
        recorded.event_id.as_deref(),
        Some(earlier.stored.id.as_str()),
        "the retry records the event the first attempt admitted, not a second one"
    );
    assert_eq!(
        admissions_for(&fixture).await,
        1,
        "exactly one event exists"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_claim_that_lapsed_mid_dispatch_is_not_recorded_as_this_passes() {
    let _lane = REPAIR_LANE.lock().await;
    let fixture = Fixture::create().await;
    fixture.seed_intent().await;
    // Another replica re-claims the intent the moment this pass's admission
    // lands — the claim lapsed while the append was in flight. It claims the
    // way a replica does, which is the only update the table's fence admits.
    let trigger = Trigger::install(
        &fixture,
        "AFTER",
        &format!(
            "UPDATE core.repair_verifications SET dispatch_claim_token = '{OTHER_REPLICA}', \
             dispatch_claimed_at = dispatch_claimed_at + 1, \
             updated_at = dispatch_claimed_at + 1, \
             dispatch_attempts = dispatch_attempts + 1 \
             WHERE id = '{}'::uuid; RETURN NULL;",
            fixture.verification
        ),
    )
    .await;
    let admissions = Admissions::for_tests(fixture.database.clone(), connect_redis().await);

    Repairs::new(fixture.database.clone(), admissions, Entropy::new())
        .sweep()
        .await
        .expect("a lost claim is a counted failure, not a faulted pass");
    trigger.remove().await;

    assert_eq!(
        fixture.verification().await.event_id,
        None,
        "a pass whose claim moved on records nothing"
    );
    assert_eq!(claim_token(&fixture).await.as_deref(), Some(OTHER_REPLICA));
    assert_eq!(
        fixture.verification().await.attempts,
        2,
        "this pass's claim, then the other replica's"
    );
    assert_eq!(
        admissions_for(&fixture).await,
        1,
        "the event itself exists once"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_dispatch_the_ledger_refuses_is_logged_and_left_to_retry() {
    let _lane = REPAIR_LANE.lock().await;
    let fixture = Fixture::create().await;
    fixture.seed_intent().await;
    let trigger = Trigger::install(
        &fixture,
        "BEFORE",
        "RAISE EXCEPTION 'the fixture refuses this admission';",
    )
    .await;
    let admissions = Admissions::for_tests(fixture.database.clone(), connect_redis().await);
    let recorder = Recorder::default();

    recorder
        .around(Repairs::new(fixture.database.clone(), admissions, Entropy::new()).sweep())
        .await
        .expect("one intent failing does not fail the pass");
    trigger.remove().await;

    let logged = recorder
        .find(EVENT_DISPATCH_FAILED, |fields| {
            fields.get("verification_id") == Some(&fixture.verification)
        })
        .expect("the refused dispatch is logged with the intent it failed");
    assert_eq!(logged.get("workspace_id"), Some(&fixture.workspace));
    assert!(logged.contains_key("due_since_ms"), "{logged:?}");
    assert_eq!(fixture.verification().await.event_id, None);
    assert!(
        claim_token(&fixture).await.is_some(),
        "the claim stays until it lapses, then the intent retries"
    );
    assert_eq!(admissions_for(&fixture).await, 0, "nothing was admitted");
    fixture.cleanup().await;
}

/// Admission rows for the fixture's verifier fleet.
async fn admissions_for(fixture: &Fixture) -> i64 {
    sqlx::query("SELECT count(*) FROM core.fleet_admissions WHERE fleet_id = $1::uuid")
        .bind(&fixture.verifier_fleet)
        .fetch_one(&mut *fixture.database.acquire().await.expect("a connection"))
        .await
        .expect("the count runs")
        .try_get(0)
        .expect("a count is a bigint")
}

/// The intent's current claim token, as text.
async fn claim_token(fixture: &Fixture) -> Option<String> {
    sqlx::query(
        "SELECT dispatch_claim_token::text FROM core.repair_verifications WHERE id = $1::uuid",
    )
    .bind(&fixture.verification)
    .fetch_one(&mut *fixture.database.acquire().await.expect("a connection"))
    .await
    .expect("the intent reads")
    .try_get(0)
    .expect("the token reads as text or null")
}

/// A row trigger on `core.fleet_admissions`, scoped to one verifier fleet.
struct Trigger {
    admin: sqlx::PgPool,
    name: String,
}

impl Trigger {
    /// Installs `body` to run `timing` each insert for the fixture's fleet.
    async fn install(fixture: &Fixture, timing: &str, body: &str) -> Self {
        let name = format!("afd_it_repair_{}", fixture.verifier_fleet.replace('-', ""));
        let admin = sqlx::PgPool::connect(&fixture.lane.url())
            .await
            .expect("the lane's owner connects");
        // Every interpolated value is this fixture's own minted id or a
        // constant in this file, never input.
        for statement in [
            format!(
                "CREATE FUNCTION public.{name}() RETURNS trigger LANGUAGE plpgsql AS $$ \
                 BEGIN {body} END $$"
            ),
            format!(
                "CREATE TRIGGER {name} {timing} INSERT ON core.fleet_admissions FOR EACH ROW \
                 WHEN (NEW.fleet_id = '{}'::uuid) EXECUTE FUNCTION public.{name}()",
                fixture.verifier_fleet
            ),
        ] {
            sqlx::query(AssertSqlSafe(statement))
                .execute(&admin)
                .await
                .expect("the injected failure installs");
        }
        Self { admin, name }
    }

    async fn remove(self) {
        for statement in [
            format!(
                "DROP TRIGGER IF EXISTS {0} ON core.fleet_admissions",
                self.name
            ),
            format!("DROP FUNCTION IF EXISTS public.{0}()", self.name),
        ] {
            let _dropped = sqlx::query(AssertSqlSafe(statement))
                .execute(&self.admin)
                .await;
        }
    }
}
