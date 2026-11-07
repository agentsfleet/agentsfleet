//! What a token may carry that nobody asked for: GitHub's own `metadata` read,
//! and nothing else at any level.

use afd_fleet_runtime::config::Access;
use serde_json::json;

use super::{Overreach, binding, granted, repositories, scoped};

#[test]
fn verify_refuses_an_unrequested_read_other_than_metadata() {
    // A read is still reach: `administration: read` shows a repository's
    // settings and collaborators, and `secrets: read` names every Actions
    // secret. Neither binding asked for either.
    for access in [Access::Read, Access::Write] {
        let binding = binding(access);
        let request = scoped(&binding);
        // A fresh grant per stranger, so each pass grants exactly one: the
        // second must be refused on its own, not beside the first.
        for stranger in ["administration", "secrets"] {
            let mut asked = serde_json::to_value(request.permissions())
                .expect("the request's permissions serialise");
            asked[stranger] = json!("read");
            assert_eq!(
                granted(asked, repositories()).verify(&binding, request.permissions()),
                Err(Overreach::Permissions),
                "{access:?} binding granted {stranger}: read"
            );
        }
    }
}

#[test]
fn verify_admits_metadata_beside_the_request() {
    // GitHub attaches `metadata: read` to every installation token, so a
    // check refusing it would refuse every mint.
    for access in [Access::Read, Access::Write] {
        let binding = binding(access);
        let request = scoped(&binding);
        let mut asked = serde_json::to_value(request.permissions())
            .expect("the request's permissions serialise");
        asked["metadata"] = json!("read");

        assert_eq!(
            granted(asked, repositories()).verify(&binding, request.permissions()),
            Ok(()),
            "{access:?} binding"
        );
    }
}

#[test]
fn verify_refuses_metadata_above_read() {
    // The admission is for the read GitHub attaches, not for the name: a
    // `metadata` grant at any other level is a stranger like the rest.
    let binding = binding(Access::Read);
    let request = scoped(&binding);
    let mut asked =
        serde_json::to_value(request.permissions()).expect("the request's permissions serialise");
    asked["metadata"] = json!("write");

    assert_eq!(
        granted(asked, repositories()).verify(&binding, request.permissions()),
        Err(Overreach::Permissions)
    );
}
