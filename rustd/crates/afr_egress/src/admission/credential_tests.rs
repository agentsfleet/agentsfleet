//! Where a credential may be sent: its own host, an origin policy naming it,
//! and nowhere at all from a tool that carries none.

use super::Placement;
use super::tests::{admit, draft, misplaced, refused};
use crate::error::{Error, raise};
use crate::fixture::{ELASTIC, GITHUB, GRAFANA, shown};
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
        Some(shown(&Error::secret_not_found("missing", "host")))
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
        Some(shown(&raise::credential_host_not_allowed(
            "grafana", ELASTIC
        )))
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
        Some(shown(&raise::credential_host_not_allowed(
            "unbound", ELASTIC
        )))
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
        Some(shown(&Error::secret_not_found("nobody", "token")))
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
        Some(shown(&raise::credential_host_not_allowed(
            "github", GRAFANA
        )))
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
        Some(shown(&Error::secret_not_found("github", "password")))
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
                Some((
                    Some(Refusal::PlacementNotAllowed | Refusal::HostNotAllowed),
                    _
                ))
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
        .err()
        .as_ref()
        .map(shown);

    assert_eq!(
        refusal,
        Some(misplaced(
            "a placeholder in the URL, or credentials in its userinfo"
        ))
    );
}
