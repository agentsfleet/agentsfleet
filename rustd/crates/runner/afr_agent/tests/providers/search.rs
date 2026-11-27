//! A real Chat wire reads the result of its search on its next model turn.

use std::sync::atomic::{AtomicUsize, Ordering};

use af_wire::policy::HttpEgress;
use af_wire::web::{SearchResponse, SearchResult};
use afr_tools::{RunVerbs, ScheduleCall, Unanswered};
use serde_json::json;

use super::support::wires::Wire;
use super::support::{Fake, engine, payload, run_with_verbs};
use super::{ANSWER, CALL_ID};

#[derive(Debug, Default)]
struct SearchPlane(AtomicUsize);

#[async_trait::async_trait]
impl RunVerbs for SearchPlane {
    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn web_search(&self, number: u64, query: &str) -> Result<SearchResponse, Unanswered> {
        assert_eq!(number, 1);
        assert_eq!(query, "tides");
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(SearchResponse {
            vendor: "parallel".into(),
            results: vec![SearchResult {
                url: "https://tides.example/".into(),
                title: Some("Tide times".into()),
                published: None,
                excerpts: vec!["High tide at noon".into()],
            }],
        })
    }
}

#[tokio::test]
async fn a_chat_model_searches_through_the_verb() {
    for (policy, calls, result) in [
        (HttpEgress::Public, 1, "High tide at noon"),
        (HttpEgress::Restricted, 0, "[network_restricted]"),
    ] {
        let wire = Wire::Chat;
        let mut fake = Fake::serve(vec![
            wire.call(CALL_ID, "web_search", &json!({"query":"tides"})),
            wire.answer(ANSWER),
        ])
        .await;
        let mut held = payload(&wire.provider(), &["web_search"], "What are the tides?");
        held.policy.network_policy.http_egress = policy;
        let plane = SearchPlane::default();
        let (output, _) = run_with_verbs(&engine(&fake), &held, None, &plane).await;
        assert_eq!(output.result.content, ANSWER);
        assert_eq!(plane.0.load(Ordering::SeqCst), calls);
        let seen = fake.seen();
        assert_eq!(wire.offered(&seen[0].body), ["web_search"]);
        let next = seen[1].body.to_string();
        assert!(next.contains(result), "{next}");
        if policy == HttpEgress::Restricted {
            assert!(next.contains("Allow all"), "{next}");
        }
    }
}

#[tokio::test]
async fn messages_and_responses_offer_search_as_a_query_function() {
    for wire in [Wire::Messages, Wire::Responses] {
        let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
        let held = payload(&wire.provider(), &["web_search"], "Find the guide");
        let (output, _) = super::support::run(&engine(&fake), &held).await;
        assert_eq!(output.result.content, ANSWER);
        let seen = fake.seen();
        let offered = seen[0].body["tools"].as_array().unwrap();
        assert_eq!(offered.len(), 1);
        let search = &offered[0];
        assert_eq!(search["name"], "web_search");
        let schema = if matches!(wire, Wire::Messages) {
            &search["input_schema"]
        } else {
            assert_eq!(search["type"], "function");
            &search["parameters"]
        };
        assert_eq!(schema["properties"]["query"]["type"], "string");
    }
}
