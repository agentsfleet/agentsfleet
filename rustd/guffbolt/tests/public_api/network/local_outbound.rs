//! `LocalOutbound`: the allowlist, and the refusals before a request leaves.

use super::*;

#[test]
fn an_allowlist_allows_a_listed_host_and_its_subdomains_in_any_case() {
    let hosts = Allowlist::Hosts(vec!["Example.com".to_owned()]);

    let judged: Vec<bool> = [
        "example.com",
        "API.example.COM",
        "a.b.example.com",
        "badexample.com",
        "example.com.evil",
        "",
    ]
    .iter()
    .map(|host| hosts.allows(host))
    .collect();

    assert_eq!(judged, [true, true, true, false, false, false]);
    assert!(Allowlist::Any.allows("anything.test"));
    assert!(
        !Allowlist::Hosts(Vec::new()).allows("example.com"),
        "an empty list allows nothing"
    );
}

/// Every request `LocalOutbound` will not send is refused before a lookup or
/// a connection: a method it does not send, a URL that is no URL, plain
/// HTTP, a private literal, and a host off the allowlist.
#[tokio::test]
async fn local_outbound_refuses_a_request_before_it_leaves() {
    let lookup = Arc::new(Fixed::default());
    let outbound = LocalOutbound::resolving(
        Allowlist::Hosts(vec!["docs.test".to_owned()]),
        Arc::<Fixed>::clone(&lookup),
    )
    .expect("the client builds");
    let tools = web(&outbound, &["http_request"]);

    let mut read = Vec::new();
    for arguments in [
        json!({"url": "https://docs.test/", "method": "TRACE"}),
        json!({"url": "not a url"}),
        json!({"url": "http://docs.test/"}),
        json!({"url": "https://127.0.0.1/"}),
        json!({"url": "https://[::ffff:10.0.0.1]/"}),
        json!({"url": "https://elsewhere.test/"}),
    ] {
        read.push(dispatch(&tools, "http_request", arguments).await.text);
    }

    assert_eq!(
        read,
        [
            "[method_not_allowed] TRACE is not a method this tool sends",
            "[invalid_arguments] not a url is not a URL: relative URL without a base",
            "[https_required] http://docs.test/ is not https",
            "[egress_denied] 127.0.0.1 is a private, loopback or reserved address",
            "[egress_denied] [::ffff:a00:1] is a private, loopback or reserved address",
            "[host_not_allowed] elsewhere.test is not an allowed host",
        ]
    );
    assert_eq!(
        lookup.asked(),
        Vec::<String>::new(),
        "no name was looked up"
    );
}

/// A name that resolves to a private address is refused whole by the
/// client's guard, after the credential was placed and before any
/// connection; a name no lookup answers reads as unreachable, with no URL.
#[tokio::test]
async fn local_outbound_refuses_a_name_that_resolves_private_or_not_at_all() {
    let lookup = Arc::new(Fixed::default().answering("inside.test", "10.0.0.7:443"));
    let outbound = LocalOutbound::resolving(Allowlist::Any, Arc::<Fixed>::clone(&lookup))
        .expect("the client builds")
        .with_secret("api", "token", "s3cret");
    let tools = web(&outbound, &["http_request"]);

    let inside = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://inside.test/v1", "method": "POST", "body": "{}",
               "headers": {"Authorization": "Bearer ${secrets.api.token}", "X-Trace": "1"}}),
    )
    .await;
    let nowhere = dispatch(
        &tools,
        "http_request",
        json!({"url": "https://nowhere.test/secret-path"}),
    )
    .await;

    assert_eq!(
        inside.text,
        "[egress_denied] inside.test resolves to a private, loopback or reserved address"
    );
    assert_eq!(nowhere.error_code, Some(code::UPSTREAM_UNREACHABLE));
    assert!(
        !nowhere.text.contains("secret-path"),
        "the refusal carries no URL: {}",
        nowhere.text
    );
    assert_eq!(lookup.asked(), ["inside.test", "nowhere.test"]);
}

#[tokio::test]
async fn local_outbound_masks_places_and_admits_as_its_secrets_and_allowlist_say() {
    let lookup = Arc::new(Fixed::default().answering("docs.test", "192.0.2.10:443"));
    let outbound = LocalOutbound::resolving(
        Allowlist::Hosts(vec!["docs.test".to_owned()]),
        Arc::<Fixed>::clone(&lookup),
    )
    .expect("the client builds")
    .with_secret("api", "token", "s3cret")
    .with_secret("api", "user", "ada")
    .with_secret("blank", "value", "");

    let masked = outbound
        .mask("ada sent s3cret, and s3cret again".to_owned())
        .await;
    let token = outbound.secret("http_request", "api", "token").await;
    let no_field = outbound.secret("http_request", "api", "password").await;
    let no_secret = outbound.secret("http_request", "db", "token").await;
    let admitted = outbound.connect("mcp", "https://docs.test/mcp").await;
    let plain = outbound.connect("mcp", "http://docs.test/mcp").await;
    let resolved: Vec<SocketAddr> = outbound
        .lookup()
        .resolve("docs.test".parse().expect("a name"))
        .await
        .expect("the fixed lookup answers")
        .collect();

    assert_eq!(
        masked, "[masked] sent [masked], and [masked] again",
        "a blank secret masks nothing"
    );
    assert_eq!(token, Ok("s3cret".to_owned()));
    assert_eq!(
        no_field,
        Err(Refused::new(
            code::SECRET_NOT_FOUND,
            "no secret api with a field password"
        ))
    );
    assert_eq!(
        no_secret,
        Err(Refused::new(
            code::SECRET_NOT_FOUND,
            "no secret db with a field token"
        ))
    );
    assert_eq!(
        admitted,
        Ok(Admitted {
            url: "https://docs.test/mcp".to_owned(),
            headers: Vec::new()
        })
    );
    assert_eq!(
        plain,
        Err(Refused::new(
            code::HTTPS_REQUIRED,
            "http://docs.test/mcp is not https"
        ))
    );
    assert_eq!(
        resolved,
        ["192.0.2.10:443".parse::<SocketAddr>().expect("an address")]
    );
    assert_eq!(
        lookup.asked(),
        ["docs.test"],
        "its lookup is the one it was built with"
    );
    assert_eq!(
        format!("{outbound:?}"),
        r#"LocalOutbound { allowlist: Hosts(["docs.test"]), .. }"#,
        "its Debug never prints a secret"
    );
}

#[test]
fn local_outbound_over_this_hosts_resolver_builds() {
    let outbound = LocalOutbound::new(Allowlist::Any).expect("the TLS backend takes the client");

    assert_eq!(
        format!("{outbound:?}"),
        "LocalOutbound { allowlist: Any, .. }"
    );
}

/// A refused request reads as its code and sentence wherever it lands.
#[test]
fn a_refusal_reads_as_a_failed_call() {
    let output = ToolOutput::from(Refused::new(code::EGRESS_DENIED, "10.0.0.1 is private"));

    assert_eq!(output.error_code, Some(code::EGRESS_DENIED));
    assert_eq!(output.text, "[egress_denied] 10.0.0.1 is private");
}
