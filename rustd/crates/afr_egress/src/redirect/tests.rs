#![expect(clippy::expect_used, reason = "test preconditions must fail loudly")]

use reqwest::Url;
use reqwest::header::{HeaderMap, HeaderValue, LOCATION};

use super::{Chain, Stop, Target, follows};
use crate::testing::{inbound, redirected};

const FIRST: &str = "https://api.github.com/start";

#[test]
fn should_bound_total_redirect_work() {
    let first = Url::parse(FIRST).expect("fixture URL");
    let mut chain = Chain::new(&first, Vec::new());
    let mut current = first;
    for index in 1..=3 {
        let mut reply = redirected(302, &current, &format!("/hop/{index}"));
        let draft = chain
            .next(&mut reply)
            .expect("bounded hop")
            .expect("continuation");
        current = Url::parse(&draft.url).expect("draft URL");
    }
    let mut final_reply = inbound(200, "actual job error");
    assert!(chain.next(&mut final_reply).expect("final body").is_none());
    assert_eq!(final_reply.followed_redirects, 3);
    let mut fourth = redirected(302, &current, "/hop/4");
    assert!(matches!(chain.next(&mut fourth), Err(Stop::HopLimit)));
}

#[test]
fn should_refuse_repeated_normalized_targets_before_the_hop_limit() {
    let first = Url::parse("https://API.GITHUB.COM:443/start#initial").expect("fixture URL");
    let mut chain = Chain::new(&first, Vec::new());
    let mut fragment = redirected(301, &first, "/start#changed");
    assert!(matches!(chain.next(&mut fragment), Err(Stop::Loop)));
    for index in 1..=3 {
        let mut next = redirected(307, &first, &format!("/different/{index}"));
        assert!(chain.next(&mut next).expect("new target").is_some());
    }
    let mut repeated = redirected(308, &first, FIRST);
    assert!(matches!(chain.next(&mut repeated), Err(Stop::Loop)));
}

#[test]
fn should_refuse_invalid_or_ambiguous_locations() {
    let base = Url::parse(FIRST).expect("fixture URL");
    for raw in ["".to_owned(), "../".repeat(2731), "https://[".to_owned()] {
        assert_eq!(
            Target::new(&base, &raw).expect_err("invalid Location"),
            Stop::InvalidTarget
        );
    }
    let prefix = "https://storage.example/";
    let long_base =
        Url::parse(&format!("{prefix}{}", "x".repeat(8190 - prefix.len()))).expect("long base");
    assert_eq!(
        Target::new(&long_base, "?q=overflow").expect_err("resolved URL exceeds the bound"),
        Stop::InvalidTarget
    );
    let mut headers = HeaderMap::new();
    assert!(Target::from_headers(&base, &headers).is_none());
    headers.append(LOCATION, HeaderValue::from_static("/one"));
    headers.append(LOCATION, HeaderValue::from_static("/two"));
    assert!(Target::from_headers(&base, &headers).is_none());
    let mut chain = Chain::new(&base, Vec::new());
    assert!(matches!(
        chain.next(&mut inbound(302, "")),
        Err(Stop::InvalidTarget)
    ));
}

#[test]
fn should_emit_one_terminal_outcome_at_its_severity() {
    let capture = afd_core::test_util::trace::Capture::install();
    let first = Url::parse(FIRST).expect("URL");
    for status in [200, 403] {
        let mut chain = Chain::new(&first, Vec::new());
        chain
            .next(&mut redirected(302, &first, "/download"))
            .expect("hop");
        chain.next(&mut inbound(status, "source")).expect("body");
        chain.finish();
    }
    let completed = capture.only("http_redirect_completed");
    assert_eq!(completed.level, tracing::Level::INFO);
    assert_eq!(completed.field("status"), Some("200"));
    let failed = capture.only("http_redirect_failed");
    assert_eq!(failed.level, tracing::Level::WARN);
    assert_eq!(failed.field("status"), Some("403"));
    assert_eq!(failed.field("hop_count"), Some("1"));
    assert_eq!(
        failed.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str())
    );
}

#[test]
fn should_drop_modeled_headers_permanently_after_an_origin_change() {
    let base = Url::parse(FIRST).expect("fixture URL");
    let mut chain = Chain::new(&base, vec![("Authorization".into(), "original".into())]);
    let mut relative = redirected(301, &base, "/same-origin");
    assert_eq!(
        chain
            .next(&mut relative)
            .expect("hop")
            .expect("draft")
            .headers
            .len(),
        1
    );
    let mut port = redirected(302, &base, "https://api.github.com:8443/other-port");
    assert_eq!(
        chain.next(&mut port).expect("hop").expect("draft").headers,
        Vec::new()
    );
    let mut back = redirected(303, &base, "/returned");
    assert_eq!(
        chain.next(&mut back).expect("hop").expect("draft").headers,
        Vec::new()
    );
    for status in [301, 302, 303, 307, 308] {
        assert!(follows(status));
    }
    for status in [200, 300, 304, 305, 400, 500] {
        assert!(!follows(status));
    }
}
