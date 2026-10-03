use afd_core::clock::{FixedClock, UnixMillis};
use reqwest::header::AUTHORIZATION;

use super::Egress;
use crate::admission::{Draft, Placement};
use crate::fixture::{BASE, BRANCH, GITHUB, GRAFANA_TOKEN, PUSHOVER_TOKEN, PUSHOVER_USER, policy};
use crate::refusal::Refusal;
use crate::testing::CountingMint;

const START: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);
const MINTED: &str = "ghs_minted_token";
const REFS: &str = "https://api.github.com/repos/acme/widgets/git/refs";
const POST: &str = "POST";
const PULLS: &str = "https://api.github.com/repos/acme/widgets/pulls";

fn post(url: &str, body: &str) -> Draft {
    Draft {
        method: POST.to_owned(),
        url: url.to_owned(),
        headers: vec![(
            "Authorization".to_owned(),
            "Bearer ${secrets.github.token}".to_owned(),
        )],
        body: Some(body.to_owned()),
        placement: Placement::Authorization,
    }
}

/// What `draft` is refused with under the `ci-repairer`-shaped policy, or the
/// `Authorization` it would carry.
async fn prepare(draft: Draft) -> Result<String, Refusal> {
    let policy = policy(false);
    let clock = FixedClock::at(START);
    let mint = CountingMint::answering(MINTED, 3_600_000, clock.clone());
    let mut egress = Egress::new(&policy, &mint, &clock);
    egress.prepare(draft).await.map(|outbound| {
        let authorization = outbound.headers().get(AUTHORIZATION);
        assert!(authorization.is_some_and(reqwest::header::HeaderValue::is_sensitive));
        authorization
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    })
}

fn refused_post(url: &str) -> Refusal {
    Refusal::RequestPolicyNotAllowed {
        host: GITHUB.to_owned(),
        method: POST.to_owned(),
        path: url.trim_start_matches("https://api.github.com").to_owned(),
    }
}

#[tokio::test]
async fn test_write_rules_admit_only_repair_branch() {
    let named = format!(r#"{{"ref":"refs/heads/{BRANCH}","sha":"abc"}}"#);

    assert_eq!(
        prepare(post(REFS, &named)).await,
        Ok(format!("Bearer {MINTED}"))
    );
    for other in [
        r#"{"ref":"refs/heads/main","sha":"abc"}"#,
        r#"{"ref":"refs/tags/v1","sha":"abc"}"#,
        r#"{"sha":"abc"}"#,
    ] {
        assert_eq!(
            prepare(post(REFS, other)).await.err(),
            Some(refused_post(REFS)),
            "{other}"
        );
    }
}

#[tokio::test]
async fn test_write_rules_require_draft_against_base() {
    let draft_against_base =
        format!(r#"{{"head":"{BRANCH}","base":"{BASE}","draft":true,"title":"fix"}}"#);
    let ready = format!(r#"{{"head":"{BRANCH}","base":"{BASE}","draft":false}}"#);
    let into_main = format!(r#"{{"head":"{BRANCH}","base":"main","draft":true}}"#);
    let other_head = format!(r#"{{"head":"main","base":"{BASE}","draft":true}}"#);

    assert_eq!(
        prepare(post(PULLS, &draft_against_base)).await,
        Ok(format!("Bearer {MINTED}"))
    );
    for refused in [ready, into_main, other_head] {
        assert_eq!(
            prepare(post(PULLS, &refused)).await.err(),
            Some(refused_post(PULLS)),
            "{refused}"
        );
    }
}

#[tokio::test]
async fn should_refuse_a_header_http_cannot_carry() {
    let mut draft = post(REFS, "{}");
    draft.headers = vec![("Bad Header".to_owned(), "x".to_owned())];
    draft.body = Some(format!(r#"{{"ref":"refs/heads/{BRANCH}"}}"#));

    assert_eq!(
        prepare(draft).await.err(),
        Some(Refusal::InvalidHeader {
            name: "Bad Header".to_owned()
        })
    );
}

#[tokio::test]
async fn should_admit_nothing_through_a_closed_guard() {
    let mut closed = Egress::closed();

    let refused = closed.prepare(post(PULLS, "{}")).await.err();

    assert_eq!(
        refused,
        Some(Refusal::HostNotAllowed {
            host: GITHUB.to_owned()
        })
    );
}

#[tokio::test]
async fn should_print_no_secret_in_its_debug_after_a_mint() {
    let policy = policy(false);
    let clock = FixedClock::at(START);
    let mint = CountingMint::answering(MINTED, 3_600_000, clock.clone());
    let mut egress = Egress::new(&policy, &mint, &clock);
    let named = format!(r#"{{"ref":"refs/heads/{BRANCH}","sha":"abc"}}"#);
    let prepared = egress.prepare(post(REFS, &named)).await;

    let printed = format!("{egress:?} {prepared:?}");

    assert_eq!(mint.asked(), 1);
    for secret in [
        MINTED,
        GRAFANA_TOKEN,
        PUSHOVER_TOKEN,
        PUSHOVER_USER,
        "es_live_key",
    ] {
        assert!(!printed.contains(secret), "{secret} printed in {printed}");
    }
}
