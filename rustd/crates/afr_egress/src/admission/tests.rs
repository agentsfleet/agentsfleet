use super::{Admission, Draft, Placement};
use crate::error::raise;
use crate::fixture::{ELASTIC_QUERY, GITHUB, Shown, policy, shown};
use crate::refusal::Refusal;

pub(super) fn draft(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Draft {
    Draft {
        method: method.to_owned(),
        url: url.to_owned(),
        headers: headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        body: body.map(str::to_owned),
        placement: Placement::Authorization,
    }
}

/// The URL `draft` is admitted to under a policy with `read_only`, or why not.
pub(super) fn admit(read_only: bool, draft: Draft) -> Result<String, Shown> {
    let policy = policy(read_only);
    Admission::new(&policy)
        .admit(draft)
        .map(|admitted| admitted.url.to_string())
        .map_err(|error| shown(&error))
}

pub(super) fn refused(read_only: bool, draft: Draft) -> Option<Shown> {
    admit(read_only, draft).err()
}

#[test]
fn should_send_only_the_listed_methods_in_any_case() {
    let url = "https://api.github.com/repos/acme/widgets/pulls";

    assert_eq!(
        admit(false, draft("get", url, &[], None)),
        Ok(url.to_owned())
    );
    assert_eq!(
        refused(false, draft("TRACE", url, &[], None)),
        Some(shown(&raise::method_not_allowed("TRACE")))
    );
}

#[test]
fn should_refuse_an_unlisted_host_and_any_scheme_but_https() {
    assert_eq!(
        refused(false, draft("GET", "https://evil.example/x", &[], None)),
        Some(shown(&raise::host_not_allowed("evil.example")))
    );
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "http://api.github.com/repos/acme/widgets/",
                &[],
                None
            )
        ),
        Some(shown(&raise::https_required()))
    );
}

#[test]
fn should_refuse_an_allowlisted_address_literal_in_a_private_range() {
    assert_eq!(
        refused(false, draft("GET", "https://127.0.0.1/admin", &[], None)),
        Some(shown(&raise::address_not_allowed("127.0.0.1")))
    );
}

#[test]
fn should_refuse_a_placeholder_anywhere_but_authorization_and_the_whole_host() {
    let url = "https://demo-grafana.internal/api/search";
    let token = "${secrets.grafana.token}";

    assert_eq!(
        refused(
            false,
            draft("POST", url, &[], Some(&format!("{{\"t\":\"{token}\"}}")))
        ),
        Some(misplaced("a placeholder in the body"))
    );
    assert_eq!(
        refused(
            false,
            draft("GET", &format!("{url}?key={token}"), &[], None)
        ),
        Some(misplaced(
            "a placeholder in the URL, or credentials in its userinfo"
        ))
    );
    assert_eq!(
        refused(false, draft("GET", url, &[("X-Api-Key", token)], None)),
        Some(misplaced("the X-Api-Key header as written"))
    );
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                url,
                &[("Authorization", "Bearer ${secrets.grafana}")],
                None
            )
        ),
        Some(misplaced("the Authorization header as written"))
    );
}

#[test]
fn should_refuse_a_host_header_and_credentials_in_the_userinfo() {
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://demo-grafana.internal/",
                &[("Host", "evil.example")],
                None
            )
        ),
        Some(misplaced("the Host header as written"))
    );
    assert_eq!(
        refused(
            false,
            draft("GET", "https://user:pass@demo-grafana.internal/", &[], None)
        ),
        Some(misplaced(
            "a placeholder in the URL, or credentials in its userinfo"
        ))
    );
}

#[test]
fn should_refuse_a_request_no_origin_rule_admits() {
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://api.github.com/repos/acme/other/pulls",
                &[],
                None
            )
        ),
        Some(shown(&raise::request_policy_not_allowed(
            GITHUB,
            "GET",
            "/repos/acme/other/pulls"
        )))
    );
}

#[test]
fn should_admit_under_read_only_reads_listed_query_posts_and_rule_admitted_posts() {
    let refs = "https://api.github.com/repos/acme/widgets/git/refs";
    let locked = r#"{"ref":"refs/heads/agentsfleet-repair/run-41","sha":"abc"}"#;

    assert_eq!(
        admit(true, draft("POST", ELASTIC_QUERY, &[], Some("{}"))),
        Ok(ELASTIC_QUERY.to_owned())
    );
    assert_eq!(
        admit(
            true,
            draft(
                "POST",
                &format!("{ELASTIC_QUERY}?format=json"),
                &[],
                Some("{}")
            )
        ),
        Ok(format!("{ELASTIC_QUERY}?format=json"))
    );
    assert_eq!(
        admit(true, draft("POST", refs, &[], Some(locked))),
        Ok(refs.to_owned())
    );
    for refused_post in [
        "https://demo.es.example/_query_other",
        "https://demo.es.example/_bulk",
    ] {
        assert_eq!(
            refused(true, draft("POST", refused_post, &[], Some("{}"))),
            Some(shown(&raise::method_not_allowed("POST"))),
            "{refused_post}"
        );
    }
    assert_eq!(
        refused(
            true,
            draft("DELETE", "https://demo.es.example/index", &[], None)
        ),
        Some(shown(&raise::method_not_allowed("DELETE")))
    );
}

#[test]
fn should_refuse_a_url_that_does_not_parse() {
    let refusal = refused(false, draft("GET", "https://[::1", &[], None));

    assert!(
        matches!(refusal, Some((Some(Refusal::InvalidUrl), _))),
        "{refusal:?}"
    );
}

#[test]
fn should_refuse_a_placeholder_in_a_header_name() {
    assert_eq!(
        refused(
            false,
            draft(
                "GET",
                "https://demo-grafana.internal/",
                &[("X-${secrets.grafana.token}", "1")],
                None
            )
        ),
        Some(misplaced(
            "the X-${secrets.grafana.token} header as written"
        ))
    );
}

#[test]
fn should_admit_a_read_under_read_only() {
    let github = "https://api.github.com/repos/acme/widgets/pulls";
    let elastic = "https://demo.es.example/index";

    assert_eq!(
        admit(true, draft("GET", github, &[], None)),
        Ok(github.to_owned())
    );
    assert_eq!(
        admit(true, draft("HEAD", elastic, &[], None)),
        Ok(elastic.to_owned())
    );
}

// `::1`, and the metadata address spelt IPv4-mapped; the allowlist names
// each as the URL parser normalises it.
#[test]
fn should_refuse_an_allowlisted_v6_literal_in_a_private_range() {
    let literals = [
        ("[::1]", "[::1]"),
        ("[::ffff:169.254.169.254]", "[::ffff:a9fe:a9fe]"),
    ];
    for (written, host) in literals {
        let mut policy = policy(false);
        policy.network_policy.allow.push(host.into());

        let url = format!("https://{written}/admin");
        let refusal = Admission::new(&policy)
            .admit(draft("GET", &url, &[], None))
            .err()
            .as_ref()
            .map(shown);

        let refused = shown(&raise::address_not_allowed(host));
        assert_eq!(refusal, Some(refused), "{written}");
    }
}

#[test]
fn should_refuse_userinfo_carrying_only_a_name_or_only_a_password() {
    for userinfo in [
        "https://user@demo-grafana.internal/",
        "https://:secret@demo-grafana.internal/",
    ] {
        assert_eq!(
            refused(false, draft("GET", userinfo, &[], None)),
            Some(misplaced(
                "a placeholder in the URL, or credentials in its userinfo"
            )),
            "{userinfo}"
        );
    }
}

/// What `what`, set where no placeholder may stand, is refused with.
pub(super) fn misplaced(what: &str) -> Shown {
    shown(&raise::placement_not_allowed(what))
}
