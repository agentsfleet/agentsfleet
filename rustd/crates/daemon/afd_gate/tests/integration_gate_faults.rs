//! Gate decisions preserve their durable result when a datastore is unavailable.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/gate_fixture.rs"]
mod fixture;

use af_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_gate::gate::{Claim, Evaluation, GateRef, Gates, Park, Parked, Stated, Verdict, Waiting};

use self::fixture::{Fixture, NOW, config};

/// How long a parked question waits for its answer.
const REFERENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_hours(1);

/// Gates over `database` and a bus that refuses every publish and subscribe.
fn gates_without_a_bus(database: afd_db::Db) -> Gates {
    Gates::new(database, afd_bus::test_util::unreachable(), Entropy::new())
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_reference_whose_row_is_gone_keeps_the_event_waiting() {
    let fixture = Fixture::create().await;
    let gates = gates_without_a_bus(fixture.database.clone());

    let evaluation = gates
        .evaluate(&GateRef::new(mint_id()))
        .await
        .expect("the durable read answers");

    assert_eq!(
        evaluation,
        Evaluation::Pending,
        "no row means no deadline to lapse; waiting is the fail-safe direction"
    );
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn parked_event_finds_its_gate_from_the_table() -> Result<(), &'static str> {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let gates = gates_without_a_bus(fixture.database.clone());
    let turn_id = &mint_id();
    let config = config(false);
    let scope = fixture.check(turn_id, &config);

    let parked = gates
        .park(
            Park {
                fleet_id: scope.fleet_id,
                workspace_id: scope.workspace_id,
                turn_id,
                stated: Stated::of("chat", "user:fixture", turn_id.as_str(), REFERENCE_TIMEOUT),
                claim: &Claim::of(None),
            },
            NOW,
        )
        .await;
    let Parked::Awaiting(reference) = parked else {
        return Err("a parked event awaits its human");
    };

    let found = gates
        .recorded(scope.fleet_id, turn_id)
        .await
        .expect("the durable read answers");

    assert_eq!(
        found.as_ref().map(GateRef::action_id),
        Some(reference.action_id()),
        "the gate's action is found from approval_gates, with no cache"
    );
    fixture.cleanup().await;
    Ok(())
}

/// The columns a role may read to find which gate an event holds, and not
/// the `status` that says how it was decided.
const GATE_REFERENCE_COLUMNS: &str = "id, action_id, fleet_id, turn_id, created_at";

/// A role, of this test alone, that can find an event's gate but not read its
/// decision: the lookup answers and the decision read that follows it fails,
/// as a lost connection between the two would.
struct DecisionBlind {
    owner: String,
    role: String,
}

impl DecisionBlind {
    /// Creates the role and lets the API login act as it.
    async fn create(fixture: &Fixture) -> Self {
        let owner = fixture.lane().url();
        let role = format!("afd_it_gate_blind_{}", mint_id().as_str().replace('-', ""));
        let login: String = sqlx::query_scalar("SELECT current_user::text")
            .fetch_one(&mut *fixture.database.acquire().await.expect("an API connection"))
            .await
            .expect("the API login names itself");
        let mut connection = <sqlx::PgConnection as sqlx::Connection>::connect(&owner)
            .await
            .expect("the lane owner connects");
        // The role is minted here and the login read from the server, never
        // input, which is what makes interpolating them safe.
        for statement in [
            format!("CREATE ROLE {role} NOLOGIN"),
            format!("GRANT USAGE ON SCHEMA fleet TO {role}"),
            format!("GRANT SELECT ({GATE_REFERENCE_COLUMNS}) ON fleet.approval_gates TO {role}"),
            format!("GRANT {role} TO \"{login}\""),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&mut connection)
                .await
                .expect("the lane owner grants the blind role");
        }
        Self { owner, role }
    }

    /// A pool that acts as the role from its first statement.
    async fn pool(&self, fixture: &Fixture) -> afd_db::Db {
        let base = fixture.lane().api_url();
        let joiner = if base.contains('?') { '&' } else { '?' };
        let url = format!("{base}{joiner}options=-c%20role%3D{}", self.role);
        let knob = afd_db::config::DbRole::Api.url_knob();
        fixture
            .lane()
            .open(afd_db::config::DbRole::Api, &[(knob, &url)])
            .await
    }

    async fn drop_role(self) {
        let mut connection = <sqlx::PgConnection as sqlx::Connection>::connect(&self.owner)
            .await
            .expect("the lane owner connects");
        for statement in [
            format!("DROP OWNED BY {}", self.role),
            format!("DROP ROLE {}", self.role),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&mut connection)
                .await
                .expect("the blind role is removed");
        }
    }
}

/// A recorded gate whose decision cannot be read keeps its event waiting: a
/// transient read must not deny a gate a human approved, and asking again
/// would raise a second card. The fault is logged under the fleet and event.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_recorded_gate_whose_decision_cannot_be_read_keeps_the_event_waiting()
-> Result<(), &'static str> {
    let capture = af_core::test_util::trace::Capture::install();
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let turn_id = &mint_id();
    let config = config(false);
    let parked = gates_without_a_bus(fixture.database.clone())
        .park(
            Park {
                fleet_id: &fixture.fleet,
                workspace_id: fixture.check(turn_id, &config).workspace_id,
                turn_id,
                stated: Stated::of("chat", "user:fixture", turn_id.as_str(), REFERENCE_TIMEOUT),
                claim: &Claim::of(None),
            },
            NOW,
        )
        .await;
    if !matches!(parked, Parked::Awaiting(_)) {
        return Err("a parked event awaits its human");
    }
    let blind = DecisionBlind::create(&fixture).await;

    let verdict = gates_without_a_bus(blind.pool(&fixture).await)
        .check(fixture.check(turn_id, &config), NOW)
        .await;

    blind.drop_role().await;
    assert_eq!(verdict, Verdict::Await(Waiting::Unreadable));
    let logged = capture.only("gate_decision_read_failed");
    assert_eq!(logged.field("fleet_id"), Some(fixture.fleet.as_str()));
    assert_eq!(logged.field("agentsfleet_turn_id"), Some(turn_id.as_str()));
    fixture.cleanup().await;
    Ok(())
}
