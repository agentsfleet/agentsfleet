//! Where a credential may be sent: its own host, an origin policy naming it,
//! and nowhere at all from a tool that carries none.

use super::Placement;
use super::tests::{admit, draft, misplaced, refused};
use crate::fixture::{ELASTIC, GITHUB, GRAFANA};
use crate::refusal::Refusal;

#[test]
fn should_put_a_credentials_own_host_in_place_of_a_whole_host_placeholder() {
    assert_eq!(
        admit(
            false,
            draft(
                "GET",
                "https://${secrets.grafana.host}/api/datasources",
                &[],
                None
            )
        ),
        Ok(format!("https://{GRAFANA}/api/datasources"))
    );
    assert_eq!(
        refused(
            false,
            draft("GET", "https://${secrets.missing.host}/x", &[], None)
        ),
        Some(Refusal::SecretNotFound {
            name: "missing".to_owned(),
            field: "host".to_owned()
        })
    );
}

#[test]
fn should_send_a_static_credential_only_to_its_own_host() {
    let grafana = [("Authorization", "Bearer ${secrets.grafana.token}")];

    assert!(
        admit(
            false,
            draft(
                "GET",
                "https://demo-grafana.internal/api/search",
                &grafana,
                None
            )
        )
        .is_ok_and(|url| url.contains(GRAFANA))
    );
    assert_eq!(
        refused(
            false,
            draft("GET", "https://demo.es.example/", &grafana, None)
        ),
        Some(Refusal::CredentialHostNotAllowed {
            name: "grafana".to_owned(),
            host: ELASTIC.to_owned()
        })
    );
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://demo.es.example/",
                &[("Authorization", "${secrets.unbound.token}")],
                None
            )
        ),
        Some(Refusal::CredentialHostNotAllowed {
            name: "unbound".to_owned(),
            host: ELASTIC.to_owned()
        })
    );
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://demo.es.example/",
                &[("Authorization", "${secrets.nobody.token}")],
                None
            )
        ),
        Some(Refusal::SecretNotFound {
            name: "nobody".to_owned(),
            field: "token".to_owned()
        })
    );
}

#[test]
fn should_send_a_minted_credential_only_where_an_origin_policy_names_it() {
    let github = [("Authorization", "Bearer ${secrets.github.token}")];

    assert!(
        admit(
            false,
            draft(
                "GET",
                "https://api.github.com/repos/acme/widgets/pulls",
                &github,
                None
            )
        )
        .is_ok_and(|url| url.contains(GITHUB))
    );
    assert_eq!(
        refused(
            false,
            draft("GET", "https://demo-grafana.internal/", &github, None)
        ),
        Some(Refusal::CredentialHostNotAllowed {
            name: "github".to_owned(),
            host: GRAFANA.to_owned()
        })
    );
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://api.github.com/repos/acme/widgets/",
                &[("Authorization", "${secrets.github.password}")],
                None
            )
        ),
        Some(Refusal::SecretNotFound {
            name: "github".to_owned(),
            field: "password".to_owned()
        })
    );
}

#[test]
fn should_refuse_every_placeholder_when_the_tool_carries_no_credential() {
    let mut fetch = draft("GET", "https://${secrets.grafana.host}/", &[], None);
    fetch.placement = Placement::Nowhere;
    let mut authorized = draft(
        "GET",
        "https://demo-grafana.internal/",
        &[("Authorization", "${secrets.grafana.token}")],
        None,
    );
    authorized.placement = Placement::Nowhere;

    assert_eq!(
        refused(false, fetch),
        Some(misplaced(
            "a placeholder in the URL, or credentials in its userinfo"
        ))
    );
    assert_eq!(
        refused(false, authorized),
        Some(misplaced("the Authorization header as written"))
    );
}

#[test]
fn should_refuse_a_host_placeholder_that_smuggles_another_host() {
    for smuggled in [
        "https://${secrets.grafana.host}:@evil.example/x",
        "https://${secrets.grafana.host}@evil.example/x",
    ] {
        let refusal = refused(false, draft("GET", smuggled, &[], None));

        assert!(
            matches!(
                refusal,
                Some(Refusal::PlacementNotAllowed { .. } | Refusal::HostNotAllowed { .. })
            ),
            "{smuggled}: {refusal:?}"
        );
    }
}

#[test]
fn should_refuse_a_secret_host_that_names_more_than_a_host() {
    let mut policy = crate::fixture::policy(false);
    if let Some(grafana) = policy
        .secrets_map
        .as_mut()
        .and_then(|secrets| secrets.get_mut("grafana"))
        .and_then(serde_json::Value::as_object_mut)
    {
        grafana.insert("host".to_owned(), "demo-grafana.internal/smuggled".into());
    }

    let refusal = super::Admission::new(&policy)
        .admit(draft(
            "GET",
            "https://${secrets.grafana.host}/api",
            &[],
            None,
        ))
        .err();

    assert_eq!(
        refusal,
        Some(misplaced(
            "a placeholder in the URL, or credentials in its userinfo"
        ))
    );
}
