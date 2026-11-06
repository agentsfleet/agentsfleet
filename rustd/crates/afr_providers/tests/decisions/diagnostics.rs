//! Performance and privacy are checked on the same real failing exchange.

use std::time::Duration;

use afd_wire::decision::DecisionApi;
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use afr_telemetry::{
    labels::decision::DecisionOutcome,
    testing::{Recorded, Tally, scoped},
};
use tracing::instrument::WithSubscriber as _;

use super::{
    fixture::{self, Reply, Server},
    trace_fixture,
};

#[tokio::test]
async fn failed_and_cancelled_calls_have_correlated_private_diagnostics() {
    let mut server = Server::start(vec![
        Reply::PrivateFailure("private-evidence-never-log".to_owned()),
        Reply::Withheld,
    ])
    .await;
    let profile = fixture::profile(&server, DecisionApi::Typesafe);
    let request = fixture::request();
    let credential = fixture::credential();
    let client = DecisionClient::with_client(server.client.clone(), profile, credential.clone());
    assert!(!format!("{client:?}").contains(credential.expose()));
    assert!(!format!("{client:?}").contains(&server.base));
    let (subscriber, logs) = trace_fixture::subscriber();
    let (tally, measured) = Tally::new();
    let error = scoped(tally, client.evaluate(&request))
        .with_subscriber(subscriber)
        .await
        .expect_err("provider failure");
    server.next().await;
    assert_failure_diagnostics(
        &logs.try_iter().collect::<Vec<_>>(),
        &error,
        credential.expose(),
        &server.base,
    );
    assert!(matches!(
        measured.try_iter().collect::<Vec<_>>().as_slice(),
        [Recorded::Decision(
            _,
            _,
            DecisionOutcome::ProviderFailure,
            _
        )]
    ));

    assert_cancelled(&client, &request, &mut server).await;
    server.shutdown().await;
}

fn assert_failure_diagnostics(
    rows: &[trace_fixture::Row],
    error: &afr_providers::Error,
    credential: &str,
    base: &str,
) {
    let span = rows
        .iter()
        .find(|row| row.name == "decision")
        .expect("decision span");
    assert_eq!(
        span.fields["agentsfleet.decision.call.id"],
        "019ed085-1300-7000-8000-000000000002"
    );
    assert_eq!(
        span.fields["agentsfleet.decision.profile_revision"],
        "reviewed-profile-1"
    );
    let events: Vec<_> = rows
        .iter()
        .filter_map(|row| row.fields.get("event").map(String::as_str))
        .collect();
    assert_eq!(
        events,
        vec![
            "dt_decision_transport_started",
            "dt_decision_transport_failed"
        ]
    );
    let failed = rows
        .iter()
        .find(|row| {
            row.fields.get("event").map(String::as_str) == Some("dt_decision_transport_failed")
        })
        .expect("failure record");
    assert_eq!(failed.fields["outcome"], "provider_failure");
    assert_eq!(
        failed.fields["decision_call_id"],
        "019ed085-1300-7000-8000-000000000002"
    );
    assert!(failed.fields.contains_key("error_code"));
    assert!(failed.fields.contains_key("elapsed_seconds"));
    let text = format!("{rows:?} {error:?}");
    for private in [
        "private-evidence-never-log",
        "Repair the importer",
        "Does the repair meet",
        credential,
        base,
    ] {
        assert!(!text.contains(private));
    }
}

async fn assert_cancelled(
    client: &DecisionClient,
    request: &garde::Valid<afd_wire::decision::DecisionRequest>,
    server: &mut Server,
) {
    let (tally, cancelled) = Tally::new();
    let (subscriber, logs) = trace_fixture::subscriber();
    scoped(tally, async {
        {
            let pending = client.evaluate(request);
            tokio::pin!(pending);
            tokio::select! {
                _ = server.next() => {},
                result = &mut pending => panic!("withheld response must remain pending: {result:?}"),
                () = tokio::time::sleep(Duration::from_secs(3)) => panic!("request did not reach owned server"),
            }
        }
    }).with_subscriber(subscriber).await;
    assert!(matches!(
        cancelled.try_iter().collect::<Vec<_>>().as_slice(),
        [Recorded::Decision(_, _, DecisionOutcome::Cancelled, _)]
    ));
    let rows: Vec<_> = logs.try_iter().collect();
    let events: Vec<_> = rows
        .iter()
        .filter_map(|row| row.fields.get("event").map(String::as_str))
        .collect();
    assert_eq!(
        events,
        [
            "dt_decision_transport_started",
            "dt_decision_transport_failed"
        ]
    );
    assert!(
        rows.iter()
            .any(|row| row.fields.get("outcome").map(String::as_str) == Some("cancelled"))
    );
    assert_eq!(
        server.unconsumed(),
        0,
        "cancellation cannot buy another call"
    );
}
