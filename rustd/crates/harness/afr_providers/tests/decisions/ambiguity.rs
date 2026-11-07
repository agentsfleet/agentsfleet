//! Ambiguous answers cannot advance work; independently readable usage survives.

use afd_wire::decision::{DecisionApi, DecisionInvalid, DecisionOutcome, DecisionUsage};
use afr_providers::decision::{DecisionClient, DecisionEvaluator as _};

use super::fixture::{self, Reply, Server};

struct Case {
    name: &'static str,
    body: String,
    reason: DecisionInvalid,
    known_usage: bool,
}

pub(super) async fn refuses_ambiguous_documents() {
    for api in [DecisionApi::Typesafe, DecisionApi::Cloudflare] {
        for case in cases(api) {
            let mut server = Server::start(vec![Reply::Bytes(case.body.into_bytes())]).await;
            let result = DecisionClient::with_client(
                server.client.clone(),
                fixture::profile(&server, api),
                fixture::credential(),
            )
            .evaluate(&fixture::request())
            .await
            .expect("syntactically readable response");
            assert_eq!(
                result.result,
                DecisionOutcome::Invalid {
                    reason: case.reason
                },
                "{}",
                case.name
            );
            assert_eq!(
                result.usage,
                case.known_usage.then_some(DecisionUsage {
                    input_tokens: 100,
                    output_tokens: 3,
                    cached_input_tokens: 0,
                    cache_write_tokens: 0,
                }),
                "{}",
                case.name
            );
            if case.reason == DecisionInvalid::Model {
                assert!(
                    result.reported_model.is_none(),
                    "ambiguous model is not evidence"
                );
            }
            server.next().await;
            assert_eq!(server.unconsumed(), 0, "ambiguity does not buy a retry");
            server.shutdown().await;
        }
    }
}

fn cases(api: DecisionApi) -> Vec<Case> {
    let body = fixture::native_reply().to_string();
    let mut cases = vec![
        Case {
            name: "conflicting question identity",
            body: body.replacen(
                "\"answers\":{",
                "\"answers\":{\"matches\":{\"type\":\"noul\",\"noul\":0.01},",
                1,
            ),
            reason: DecisionInvalid::Answers,
            known_usage: true,
        },
        Case {
            name: "conflicting probability",
            body: body.replacen("\"WAIT\":0.98", "\"WAIT\":0.01,\"WAIT\":0.98", 1),
            reason: DecisionInvalid::Answers,
            known_usage: true,
        },
        Case {
            name: "ambiguous measured usage",
            body: body.replacen(
                "\"input_tokens\":100",
                "\"input_tokens\":1,\"input_tokens\":100",
                1,
            ),
            reason: DecisionInvalid::Usage,
            known_usage: false,
        },
        Case {
            name: "ambiguous reported model",
            body: format!(
                "{{\"model\":\"unreviewed-model\",{}",
                body.trim_start_matches('{')
            ),
            reason: DecisionInvalid::Model,
            known_usage: true,
        },
    ];
    if api == DecisionApi::Cloudflare {
        for case in &mut cases {
            case.body = format!(
                "{{\"success\":true,\"result\":{},\"errors\":[]}}",
                case.body
            );
        }
        cases.push(Case {
            name: "ambiguous provider envelope",
            body: format!("{{\"success\":true,\"result\":{{}},\"result\":{body}}}"),
            reason: DecisionInvalid::Model,
            known_usage: false,
        });
    }
    cases
}
