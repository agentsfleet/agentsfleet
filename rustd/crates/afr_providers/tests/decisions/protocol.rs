//! Whole protocol exchanges retain input, typed answers and actual cache usage.

use std::collections::BTreeMap;

use afd_wire::decision::{DecisionAnswer, DecisionApi, DecisionOutcome, DecisionUsage};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};
use afr_telemetry::{
    labels::decision::{DecisionOutcome as MeasuredOutcome, DecisionProvider, DecisionPurpose},
    testing::{Recorded, Tally, scoped},
};
use serde_json::json;

use super::fixture::{self, Reply, Server};

#[tokio::test]
async fn native_protocols_preserve_complete_questions_and_aggregate_usage() {
    for (api, expected_provider) in [
        (DecisionApi::Typesafe, DecisionProvider::Typesafe),
        (DecisionApi::Perplexity, DecisionProvider::Perplexity),
        (DecisionApi::Cloudflare, DecisionProvider::Cloudflare),
    ] {
        let document = fixture::native_reply();
        let reply = if api == DecisionApi::Cloudflare {
            json!({"success":true, "result":document, "errors":[]})
        } else {
            document
        };
        let mut server = Server::start(vec![Reply::Json(reply)]).await;
        let profile = fixture::profile(&server, api);
        let request = fixture::request();
        let (tally, measurements) = Tally::new();
        let result = scoped(
            tally,
            DecisionClient::with_client(server.client.clone(), profile, fixture::credential())
                .evaluate(&request),
        )
        .await
        .expect("native evaluation");
        let seen = server.next().await;
        assert_native_exchange(&seen, &result);
        assert_measurement(&measurements, expected_provider);
        assert_eq!(server.unconsumed(), 0, "one reserved evaluation sends once");
        assert!(seen.path.ends_with(if api == DecisionApi::Typesafe {
            "/v1/systemone"
        } else if api == DecisionApi::Perplexity {
            "/v1/decisions"
        } else {
            "/ai/run/@cf/cloudflare/clef"
        }));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn openai_uses_native_named_arrays_and_retains_cache_subsets() {
    let mut server = Server::start(vec![Reply::Json(json!({
        "model":"evaluated-model-1", "usage":{"input_tokens":100, "output_tokens":3, "input_tokens_details":{"cached_tokens":20, "cache_write_tokens":10}},
        "answers":[
            {"name":"risk", "type":"score", "score":0.2, "confidence":0.8, "probabilities":[{"value":0,"label":"0","probability":0.8},{"value":1,"label":"1","probability":0.2}]},
            {"name":"matches", "type":"predicate", "probability":0.99},
            {"name":"next", "type":"choice", "choice":"WAIT", "confidence":0.96, "probabilities":[{"value":"WAIT","probability":0.98},{"value":"DONE","probability":0.02}]}
        ]
    }))]).await;
    let profile = fixture::profile(&server, DecisionApi::Openai);
    let result = DecisionClient::with_client(server.client.clone(), profile, fixture::credential())
        .evaluate(&fixture::request())
        .await
        .expect("native OpenAI evaluation");
    let seen = server.next().await;
    assert_eq!(seen.path, "/v1/decisions");
    assert_eq!(
        seen.body,
        json!({"model":"requested-model", "input":[{"role":"user","content":[{"type":"input_json","data":{"accepted_request":"Repair the importer","head":"head-b","checks":{"unit":"passed","integration":"pending"}}}]}],"questions":[
            {"name":"matches","type":"predicate","instructions":"Does the repair meet the accepted scope?\nTrue: The requested defect is fixed.\nFalse: The defect remains or scope changed."},
            {"name":"next","type":"choice","instructions":"Choose the next action.","choices":[{"value":"WAIT","description":"A required check is pending."},{"value":"DONE","description":"Every required check passed."}]},
            {"name":"risk","type":"score","instructions":"Rate the patch risk.","levels":[{"label":"0","description":"Low"},{"label":"1","description":"High"}]}
        ]})
    );
    assert_eq!(
        result.usage,
        Some(DecisionUsage {
            input_tokens: 100,
            output_tokens: 3,
            cached_input_tokens: 20,
            cache_write_tokens: 10
        })
    );
    assert_assessment(&result.result);
    assert_eq!(server.unconsumed(), 0);
    server.shutdown().await;
}

fn assert_native_exchange(
    seen: &super::fixture::Seen,
    result: &afd_wire::decision::DecisionResult,
) {
    assert_eq!(
        seen.body,
        json!({"model":"requested-model", "state":{"accepted_request":"Repair the importer", "head":"head-b", "checks":{"unit":"passed", "integration":"pending"}}, "questions":{
            "matches":{"type":"noul", "instructions":"Does the repair meet the accepted scope?", "criteria":{"true":"The requested defect is fixed.", "false":"The defect remains or scope changed."}},
            "next":{"type":"choice", "instructions":"Choose the next action.", "criteria":{"WAIT":"A required check is pending.", "DONE":"Every required check passed."}},
            "risk":{"type":"score", "instructions":"Rate the patch risk.", "criteria":["Low", "High"]}
        }})
    );
    assert_eq!(
        seen.headers["authorization"],
        format!("Bearer {}", fixture::credential().expose())
    );
    assert!(
        !seen
            .body
            .to_string()
            .contains(fixture::credential().expose())
    );
    assert_eq!(
        result.usage,
        Some(DecisionUsage {
            input_tokens: 100,
            output_tokens: 3,
            cached_input_tokens: 0,
            cache_write_tokens: 0
        })
    );
    assert_eq!(result.reported_model.as_deref(), Some("evaluated-model-1"));
    assert_assessment(&result.result);
}

fn assert_assessment(result: &DecisionOutcome) {
    assert_eq!(
        result,
        &DecisionOutcome::Answered {
            answers: BTreeMap::from([
                ("matches".to_owned(), DecisionAnswer::Noul { noul: 0.99 }),
                (
                    "next".to_owned(),
                    DecisionAnswer::Choice {
                        choice: "WAIT".to_owned(),
                        confidence: Some(0.96),
                        probabilities: BTreeMap::from([
                            ("WAIT".to_owned(), 0.98),
                            ("DONE".to_owned(), 0.02)
                        ]),
                    }
                ),
                (
                    "risk".to_owned(),
                    DecisionAnswer::Score {
                        score: 0.2,
                        confidence: Some(0.8),
                        probabilities: BTreeMap::from([(0, 0.8), (1, 0.2)]),
                    }
                ),
            ]),
        }
    );
}

fn assert_measurement(
    measurements: &std::sync::mpsc::Receiver<Recorded>,
    expected: DecisionProvider,
) {
    let measured: Vec<_> = measurements.try_iter().collect();
    assert!(
        matches!(measured.as_slice(), [Recorded::Decision(provider, DecisionPurpose::Goal, MeasuredOutcome::Completed, _)] if *provider == expected)
    );
}
