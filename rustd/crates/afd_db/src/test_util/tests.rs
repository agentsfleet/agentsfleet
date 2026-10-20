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
    use afd_core::env::EnvSource;
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
