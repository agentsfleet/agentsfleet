use std::sync::atomic::{AtomicUsize, Ordering};

use af_egress::testing::{RecordingTransport, inbound, redirected};
use af_wire::{
    policy::HttpEgress,
    web::{ExtractResponse, SearchResponse, SearchResult},
};
use async_trait::async_trait;

use super::{Egressed, NETWORK_RESTRICTED, Unanswered, WebUnanswered};
use crate::testing::RunFixture;
use crate::verbs::{RunVerbs, ScheduleCall};
use af_core::error_code;

const START: &str = "https://demo.es.example/start";
const UNKNOWN: &str = "https://docs.example.org/guide";
const DESTINATION: &str = "https://cdn.example.org/private?sig=sentinel";
const CALL: &str = "fetch-1";

#[path = "web_search_tests.rs"]
mod search;

#[derive(Debug)]
struct Verbs {
    fallback: Result<(), WebUnanswered>,
    calls: AtomicUsize,
    delay: std::time::Duration,
}

impl Verbs {
    fn new(fallback: Result<(), WebUnanswered>) -> Self {
        Self {
            fallback,
            calls: AtomicUsize::new(0),
            delay: std::time::Duration::ZERO,
        }
    }
}

#[async_trait]
impl RunVerbs for Verbs {
    fn web_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(13)
    }

    async fn schedules(&self, _: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn message(&self, _: u64, _: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }
    async fn web_search(&self, number: u64, query: &str) -> Result<SearchResponse, Unanswered> {
        assert_eq!((number, query), (8, "migration"));
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        let row = SearchResult {
            url: UNKNOWN.to_owned(),
            title: Some("Migration Guide".to_owned()),
            published: None,
            excerpts: vec![format!("Guide {}", af_egress::fixture::GRAFANA_TOKEN)],
        };
        Ok(SearchResponse {
            vendor: "parallel".to_owned(),
            results: vec![
                SearchResult {
                    url: "file:///private".to_owned(),
                    ..row.clone()
                },
                row,
            ],
        })
    }
    async fn web_extract(&self, number: u64, url: &str) -> Result<ExtractResponse, WebUnanswered> {
        assert_eq!(number, 8);
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        self.fallback?;
        Ok(ExtractResponse {
            vendor: "parallel".to_owned(),
            url: url.to_owned(),
            title: None,
            content: "extracted".to_owned(),
        })
    }
}

#[tokio::test]
async fn an_unlisted_host_goes_to_extract_only_on_a_public_fleet() {
    let mut fixture = RunFixture::new();
    let verbs = Verbs::new(Ok(()));
    let (transport, sent) = RecordingTransport::replying(200, "local");
    for mode in [HttpEgress::Restricted, HttpEgress::Public] {
        fixture.policy.network_policy.http_egress = mode;
        let run = fixture.run_reaching(&verbs);
        run.calls.open(CALL, 8);
        let result = Egressed::new(Some(&transport), &run)
            .fetch(CALL, UNKNOWN)
            .await;
        if mode == HttpEgress::Restricted {
            let Err(why) = result else {
                unreachable!("the destination must be refused")
            };
            assert_eq!(why.code, NETWORK_RESTRICTED);
            assert!(
                why.detail.contains("request_access") && why.detail.contains("docs.example.org")
            );
            assert_eq!(verbs.calls.load(Ordering::SeqCst), 0);
        } else {
            let Ok((page, final_origin)) = result else {
                unreachable!("public extraction must succeed")
            };
            assert_eq!(page.body, "extracted");
            assert_eq!(final_origin, None);
            assert_eq!(verbs.calls.load(Ordering::SeqCst), 1);
        }
    }
    assert_eq!(sent.try_iter().count(), 0);
}

#[tokio::test]
async fn an_allowlisted_page_is_read_locally_through_its_redirects() {
    let mut fixture = RunFixture::new();
    let verbs = Verbs::new(Ok(()));
    for mode in [HttpEgress::Restricted, HttpEgress::Public] {
        fixture.policy.network_policy.http_egress = mode;
        let run = fixture.run_reaching(&verbs);
        run.calls.open(CALL, 8);
        let (transport, sent) = RecordingTransport::answering(|request| {
            Ok(match request.url().path() {
                "/start" => redirected(301, request.url(), "/moved"),
                "/moved" => redirected(302, request.url(), DESTINATION),
                _ => inbound(200, "verified page"),
            })
        });
        let result = Egressed::new(Some(&transport), &run)
            .fetch(CALL, START)
            .await;
        let visits = sent.try_iter().collect::<Vec<_>>();
        assert!(visits.iter().all(|call| call.headers.is_empty()));
        if mode == HttpEgress::Restricted {
            let Err(why) = result else {
                unreachable!("the destination must be refused")
            };
            assert!(
                why.detail.contains("cdn.example.org") && why.detail.contains("request_access")
            );
            assert!(!why.detail.contains("sentinel"));
            assert_eq!(visits.len(), 2);
        } else {
            let Ok((page, origin)) = result else {
                unreachable!("public local read must succeed")
            };
            assert_eq!(page.body, "verified page");
            assert_eq!(origin.as_deref(), Some("https://cdn.example.org"));
            assert_eq!(visits.len(), 3);
        }
        assert_eq!(verbs.calls.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn an_extract_outage_falls_back_to_a_local_read() {
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    for failure in [
        WebUnanswered::LocalReadPermitted,
        WebUnanswered::Unanswered(Unanswered::Unreachable),
        WebUnanswered::Unanswered(Unanswered::Refused(Some(
            error_code::WEB_NETWORK_RESTRICTED,
        ))),
    ] {
        let verbs = Verbs::new(Err(failure));
        let run = fixture.run_reaching(&verbs);
        run.calls.open(CALL, 8);
        let (transport, sent) = RecordingTransport::replying(200, "local read");
        let result = Egressed::new(Some(&transport), &run)
            .fetch(CALL, UNKNOWN)
            .await;
        assert_eq!(verbs.calls.load(Ordering::SeqCst), 1);
        let authorized = failure == WebUnanswered::LocalReadPermitted;
        assert_eq!(result.is_ok(), authorized);
        assert_eq!(sent.try_iter().count(), usize::from(authorized));
    }
}

#[tokio::test]
async fn protected_input_and_private_addresses_reach_no_vendor() {
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    let verbs = Verbs::new(Err(WebUnanswered::LocalReadPermitted));
    let run = fixture.run_reaching(&verbs);
    run.calls.open(CALL, 8);
    let (transport, sent) = RecordingTransport::replying(200, "");
    let web = Egressed::new(Some(&transport), &run);
    let private = format!("{UNKNOWN}?x={}", af_egress::fixture::GRAFANA_TOKEN);
    for url in [
        "https://10.0.0.1/",
        "https://name:pass@docs.example.org/",
        &private,
    ] {
        let Err(refused) = web.fetch(CALL, url).await else {
            unreachable!("protected input reached the page reader")
        };
        assert!(!refused.detail.contains(af_egress::fixture::GRAFANA_TOKEN));
    }
    assert_eq!(verbs.calls.load(Ordering::SeqCst), 0);
    assert_eq!(sent.try_iter().count(), 0);
}

#[derive(Debug, Default)]
struct SlowPage(AtomicUsize);

#[async_trait]
impl af_egress::Transport for SlowPage {
    async fn send(&self, _: af_egress::Outbound) -> af_egress::Result<af_egress::Inbound> {
        self.0.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        Ok(inbound(200, "too late"))
    }
}

#[tokio::test(start_paused = true)]
async fn a_local_read_can_spend_only_the_time_left_after_extraction() {
    let mut fixture = RunFixture::new();
    fixture.policy.network_policy.http_egress = HttpEgress::Public;
    for (delay, expected_connections) in [(11, 1), (13, 0), (14, 0)] {
        let mut verbs = Verbs::new(Err(WebUnanswered::LocalReadPermitted));
        verbs.delay = std::time::Duration::from_secs(delay);
        let run = fixture.run_reaching(&verbs);
        let transport = SlowPage::default();
        run.calls.open(CALL, 8);
        let start = tokio::time::Instant::now();
        let result = Egressed::new(Some(&transport), &run)
            .fetch(CALL, UNKNOWN)
            .await;
        let Err(refused) = result else {
            unreachable!("the page reader exceeded its deadline")
        };
        assert_eq!(refused.detail, super::super::READ_DEADLINE_EXCEEDED);
        assert_eq!(start.elapsed(), verbs.web_timeout());
        assert_eq!(transport.0.load(Ordering::SeqCst), expected_connections);
        assert_eq!(verbs.calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn vendor_results_hide_protected_addresses_and_mask_excerpts() -> af_egress::Result<()> {
    let fixture = RunFixture::new();
    let run = fixture.run();
    let mut guard = run.egress.lock().await;
    let token = af_egress::fixture::GRAFANA_TOKEN;
    let mut result = SearchResult {
        url: format!("{UNKNOWN}?private={token}"),
        title: None,
        published: None,
        excerpts: vec![format!("excerpt {token}")],
    };
    assert!(super::clean_result(&mut guard, result.clone())?.is_none());
    result.url = UNKNOWN.to_owned();
    result.title = Some("https://docs.example/private?token=written".to_owned());
    let Some(safe) = super::clean_result(&mut guard, result)? else {
        unreachable!("the public URL has no protected value")
    };
    assert_eq!(safe.url, UNKNOWN);
    assert_eq!(safe.title, None);
    assert_eq!(safe.excerpts, ["excerpt «secret:grafana.token»"]);
    let signed = format!("{UNKNOWN}?token=page-signature");
    let result = SearchResult {
        url: signed.clone(),
        title: Some("page-signature".to_owned()),
        published: None,
        excerpts: vec![format!("see {signed}")],
    };
    let Some(redacted) = super::clean_result(&mut guard, result)? else {
        unreachable!("the requested path can be disclosed")
    };
    assert_eq!(redacted.url, UNKNOWN);
    assert_eq!(redacted.title, None);
    assert_eq!(redacted.excerpts, [format!("see {UNKNOWN}")]);

    Ok(())
}
