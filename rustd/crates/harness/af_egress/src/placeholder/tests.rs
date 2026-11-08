use super::{SecretRef, host_url, mentions, parse, substitute};

#[test]
fn should_parse_every_placeholder_with_its_name_and_field() {
    let parsed = parse("Bearer ${secrets.github.token} and ${secrets.elastic.api_key}");

    assert_eq!(
        parsed,
        Some(vec![
            SecretRef {
                name: "github",
                field: "token"
            },
            SecretRef {
                name: "elastic",
                field: "api_key"
            },
        ])
    );
}

#[test]
fn should_refuse_text_that_opens_a_placeholder_the_grammar_does_not_match() {
    for text in [
        "${secrets.github}",
        "${secrets.github.token",
        "${secrets.9bad.token}",
        "Bearer ${secrets.github.token} ${secrets.}",
    ] {
        assert_eq!(parse(text), None, "{text}");
        assert!(mentions(text), "{text}");
    }
}

#[test]
fn should_parse_text_with_no_placeholder_as_none_found() {
    assert_eq!(parse("Bearer plain"), Some(Vec::new()));
    assert!(!mentions("Bearer plain"));
}

#[test]
fn should_read_the_credential_a_url_host_names() {
    assert_eq!(
        host_url("https://${secrets.grafana.host}/api/datasources?x=1"),
        Some(("grafana", "/api/datasources?x=1"))
    );
    assert_eq!(
        host_url("https://${secrets.grafana.host}"),
        Some(("grafana", ""))
    );
    assert_eq!(
        host_url("https://${secrets.grafana.host}:8443/x"),
        Some(("grafana", ":8443/x"))
    );
}

#[test]
fn should_not_read_a_host_from_a_placeholder_that_is_not_the_whole_host() {
    for url in [
        "https://api.${secrets.grafana.host}/x",
        "https://${secrets.grafana.host}.evil.example/x",
        "https://${secrets.grafana.token}/x",
        "http://${secrets.grafana.host}/x",
        "https://example.com/${secrets.grafana.host}",
    ] {
        assert_eq!(host_url(url), None, "{url}");
    }
}

#[test]
fn should_substitute_each_placeholder_with_its_value() {
    let substituted = substitute("Bearer ${secrets.github.token}", |secret| {
        (secret.name == "github" && secret.field == "token").then_some("ghs_live")
    });

    assert_eq!(substituted, "Bearer ghs_live");
}

#[test]
fn should_leave_an_unanswered_placeholder_as_written() {
    let substituted = substitute("${secrets.github.token}", |_secret| None);

    assert_eq!(substituted, "${secrets.github.token}");
}
