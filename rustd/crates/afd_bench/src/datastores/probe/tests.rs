//! Redis topology parsing guards.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use super::{is_cluster_disabled, replica_label, replication_hosts};

#[test]
fn only_the_exact_cluster_disabled_detail_is_standalone() {
    assert!(is_cluster_disabled(
        "ResponseError: This instance has cluster support disabled"
    ));
    assert!(!is_cluster_disabled(
        "ResponseError: NOPERM this user has no permissions"
    ));
    assert!(!is_cluster_disabled("ResponseError: unexpected reply"));
}

#[test]
fn every_advertised_replica_address_is_returned() {
    let raw = "role:master\nconnected_slaves:2\nslave0:ip=127.0.0.1,port=6380,state=online\nreplica1:ip=::1,port=6381,state=online\n";
    assert_eq!(
        replication_hosts(raw).expect("complete replication topology"),
        vec!["127.0.0.1", "::1"]
    );
    assert!(replica_label("slave0"));
    assert!(replica_label("replica12"));
}

#[test]
fn a_missing_replica_address_fails_closed() {
    let raw = "role:master\nconnected_slaves:1\nslave0:port=6380,state=online\n";
    replication_hosts(raw).expect_err("a hidden replica address must fail closed");
}
