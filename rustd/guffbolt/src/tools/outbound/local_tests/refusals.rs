//! Every way `LocalOutbound` refuses a private address: a literal, a name
//! that resolves to one, and a redirect to either.

use super::*;

#[tokio::test]
async fn local_outbound_refuses_a_name_resolving_to_loopback() {
    let inside = Counted::listen().await;
    let lookup = Arc::new(FixedLookup::default().answering("loopback.test", &[inside.address]));
    let outbound = LocalOutbound::resolving(Allowlist::Any, Arc::<FixedLookup>::clone(&lookup))
        .expect("the client builds");

    let refused = sent(&outbound, "https://loopback.test/").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "loopback.test resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(lookup.asked(), ["loopback.test"]);
    assert_eq!(inside.connections(), 0, "no connection was opened");
}

#[tokio::test]
async fn local_outbound_refuses_a_name_whose_system_answer_is_loopback() {
    let inside = Counted::listen().await;
    let outbound = LocalOutbound::new(Allowlist::Any).expect("the client builds");
    let url = format!("https://localhost:{}/", inside.address.port());

    let refused = sent(&outbound, &url).await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "localhost resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(inside.connections(), 0, "no connection was opened");
}

#[tokio::test]
async fn local_outbound_refuses_a_private_literal_before_any_lookup() {
    let lookup = Arc::new(FixedLookup::default());
    let listed = Allowlist::Hosts(vec!["10.0.0.1".to_owned(), "[::1]".to_owned()]);
    let outbound = LocalOutbound::resolving(listed, Arc::<FixedLookup>::clone(&lookup))
        .expect("the client builds");

    for (url, host) in [
        ("https://10.0.0.1/", "10.0.0.1"),
        ("https://[::1]/", "[::1]"),
        ("https://[fd00::1]/", "[fd00::1]"),
        (
            "https://169.254.169.254/latest/meta-data",
            "169.254.169.254",
        ),
    ] {
        assert_eq!(
            sent(&outbound, url).await,
            Err(Refused::new(
                code::EGRESS_DENIED,
                format!("{host} is a private, loopback or reserved address"),
            )),
            "{url}"
        );
    }
    assert_eq!(lookup.asked(), Vec::<String>::new());
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_private_address() {
    let to_private = redirect("https://10.0.0.1/");
    let (address, root, seen) = fake(&[("/start", to_private.as_str())]).await;
    let lookup = Arc::new(FixedLookup::default());
    let outbound = LocalOutbound::routed(Arc::clone(&lookup), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "10.0.0.1 is a private, loopback or reserved address",
        ))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
    assert_eq!(
        lookup.asked(),
        Vec::<String>::new(),
        "the literal was never looked up"
    );
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_name_resolving_privately() {
    let to_private = redirect("https://private.test/");
    let (address, root, seen) = fake(&[("/start", to_private.as_str())]).await;
    let inside = Counted::listen().await;
    let lookup = Arc::new(FixedLookup::default().answering("private.test", &[inside.address]));
    let outbound = LocalOutbound::routed(Arc::clone(&lookup), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(
            code::EGRESS_DENIED,
            "private.test resolves to a private, loopback or reserved address",
        ))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
    assert_eq!(lookup.asked(), ["private.test"]);
    assert_eq!(
        inside.connections(),
        0,
        "the private host got no connection"
    );
}

#[tokio::test]
async fn local_outbound_refuses_a_redirect_to_a_location_it_cannot_read() {
    let nowhere = redirect("https://");
    let (address, root, seen) = fake(&[("/start", nowhere.as_str())]).await;
    let outbound = LocalOutbound::routed(Arc::new(FixedLookup::default()), PUBLIC, address, root);

    let refused = sent(&outbound, "https://public.test/start").await;

    assert_eq!(
        refused,
        Err(Refused::new(code::INVALID_ARGUMENTS, "empty host"))
    );
    assert_eq!(requests(&seen), ["GET /start"]);
}
