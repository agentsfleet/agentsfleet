//! Correction limits arrive with the accepted attempt, independently of host defaults.

use super::{ack, json, lease, result, rig, run};
use crate::client::Verb;

#[tokio::test]
async fn zero_corrections_stops_after_the_first_refused_proposal() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => result(
            call,
            &json!([{"id":"bad-1", "reason":"instructions_missing"}]),
            false,
        ),
        Verb::AuthoringReport => ack(),
        other => panic!("a refused proposal acquired authority {other:?}"),
    });
    let mut accepted = lease();
    accepted.limits.max_proposal_corrections = 0;
    let (calls, report) = run(&mut rig, accepted).await;
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.verb == Verb::AuthoringModel)
            .count(),
        1
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 1);
}
