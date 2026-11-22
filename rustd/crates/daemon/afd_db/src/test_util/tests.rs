//! Where an owned test database's URL points, for every lane URL shape.

use super::TestDatabase;
use crate::config::DbRole;

/// The API login's lane URL every case below pairs with its owner URL.
const API_LANE: &str = "postgres://api:secret@h:5432/lane?sslmode=require";

/// A handle that owns `name` on the lane at `base_url`.
fn owned(base_url: &str, name: &str) -> TestDatabase {
    TestDatabase {
        base_url: base_url.to_owned(),
        api_base_url: API_LANE.to_owned(),
        owned: Some(name.to_owned()),
    }
}

#[test]
fn the_database_is_swapped_and_the_query_kept() {
    // pin test: literal is the contract
    assert_eq!(
        owned("postgres://u:p@h:5432/lane?sslmode=require", "afd_it_1_0").url(),
        "postgres://u:p@h:5432/afd_it_1_0?sslmode=require"
    );
}

/// A query naming a path has a later `/` than the database's; the swap must
/// not land inside it and leave the test on the shared lane database.
#[test]
fn a_query_carrying_a_path_is_kept_whole() {
    // pin test: literal is the contract
    assert_eq!(
        owned("postgres://u@h/lane?sslrootcert=/etc/ca.pem", "afd_it_1_0").url(),
        "postgres://u@h/afd_it_1_0?sslrootcert=/etc/ca.pem"
    );
}

/// A lane URL naming no database still keeps its host.
#[test]
fn a_lane_url_with_no_database_keeps_its_host() {
    // pin test: literal is the contract
    assert_eq!(
        owned("postgres://u:p@h:5432", "afd_it_1_0").url(),
        "postgres://u:p@h:5432/afd_it_1_0"
    );
}

#[test]
fn a_shared_handle_answers_the_lane_url_unchanged() {
    let shared = TestDatabase {
        base_url: "postgres://u@h/lane?sslmode=disable".to_owned(),
        api_base_url: API_LANE.to_owned(),
        owned: None,
    };
    assert_eq!(shared.url(), "postgres://u@h/lane?sslmode=disable");
    assert_eq!(shared.api_url(), API_LANE);
}

/// An owned database is reached through the API login too, on the same
/// database the owner created.
#[test]
fn the_api_login_reaches_the_owned_database() {
    // pin test: literal is the contract
    assert_eq!(
        owned("postgres://u:p@h:5432/lane", "afd_it_1_0").api_url(),
        "postgres://api:secret@h:5432/afd_it_1_0?sslmode=require"
    );
}

/// The API role connects as the API login, as production's does; every other
/// role keeps the lane's owner.
#[test]
fn only_the_api_role_connects_as_the_api_login() {
    use af_core::env::EnvSource;
    let handle = owned("postgres://u:p@h:5432/lane", "afd_it_1_0");
    let env = handle.env(&[]);
    for role in DbRole::ALL {
        let expected = if *role == DbRole::Api {
            handle.api_url()
        } else {
            handle.url()
        };
        assert_eq!(
            env.get(role.url_knob()).as_deref(),
            Some(expected.as_str()),
            "{role:?}"
        );
    }
}

#[test]
#[should_panic(expected = "TEST_DATABASE_URL must be a URL")]
fn a_lane_url_that_is_not_a_url_is_a_lane_fault() {
    let _url = owned("not a url", "afd_it_1_0").url();
}

/// A pool on a port nobody listens on, so every statement a seed runs fails.
fn nowhere() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(50))
        .connect_lazy("postgres://seed:secret@127.0.0.1:1/agentsfleet")
        .unwrap_or_else(|failure| panic!("the fixture URL is well formed: {failure}"))
}

/// A seed whose insert fails answers the query class naming what it seeds,
/// so a suite's setup failure reads as the row it could not write.
#[tokio::test]
async fn a_seed_that_cannot_write_its_row_names_the_row() {
    use super::{
        ApiKeySeed, FleetSeed, RunnerPolicy, RunnerSeed, SEEDED_TURN, TenantSeed, TurnSeed,
        UserSeed, WorkspaceSeed, mint_id,
    };

    let pool = nowhere();
    let (id, tenant) = (mint_id(), mint_id());
    let policy = RunnerPolicy {
        sandbox_tier: "dev_none",
        network_policy: "deny_all_egress",
    };
    let refused = [
        ("seed a tenant", TenantSeed::new(&id).insert(&pool).await),
        (
            "seed a workspace",
            WorkspaceSeed::new(&id, &tenant).insert(&pool).await,
        ),
        (
            "seed an API key",
            ApiKeySeed::new(&id, &tenant, "hash", "user_fixture")
                .insert(&pool)
                .await,
        ),
        (
            "seed a person",
            UserSeed::new(&id, &tenant, "user_fixture", "seed@example.test")
                .insert(&pool)
                .await,
        ),
        (
            "seed a fleet",
            FleetSeed::new(&id, &tenant, &tenant, "active")
                .insert(&pool)
                .await,
        ),
        (
            "seed a runner",
            RunnerSeed::new(&id, "active", policy).insert(&pool).await,
        ),
        (
            "seed a fleet event",
            TurnSeed::new(&id, &tenant, &id, SEEDED_TURN)
                .insert(&pool)
                .await
                .map(drop),
        ),
    ];

    for (context, answered) in refused {
        let error = answered
            .err()
            .unwrap_or_else(|| panic!("{context} wrote a row"));
        assert!(error.is_query(), "{context}: {error}");
        assert!(error.to_string().contains(context), "{context}: {error}");
    }
}
