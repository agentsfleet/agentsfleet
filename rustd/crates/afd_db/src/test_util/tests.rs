//! Where an owned test database's URL points, for every lane URL shape.

use super::TestDatabase;

/// A handle that owns `name` on the lane at `base_url`.
fn owned(base_url: &str, name: &str) -> TestDatabase {
    TestDatabase {
        base_url: base_url.to_owned(),
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
        owned: None,
    };
    assert_eq!(shared.url(), "postgres://u@h/lane?sslmode=disable");
}

#[test]
#[should_panic(expected = "TEST_DATABASE_URL must be a URL")]
fn a_lane_url_that_is_not_a_url_is_a_lane_fault() {
    let _url = owned("not a url", "afd_it_1_0").url();
}
