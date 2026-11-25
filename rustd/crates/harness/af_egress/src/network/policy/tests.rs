//! A public HTTP policy check still crosses the production resolver's address guard.

#![expect(clippy::expect_used, reason = "test preconditions fail by panicking")]
#![expect(
    clippy::disallowed_methods,
    reason = "the test passes a raw client builder to the production address guard"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use af_wire::policy::HttpEgress;
use reqwest::header::HeaderMap;

use crate::fixture::policy;
use crate::network::Network;
use crate::policy::{Draft, EgressPolicy, Placement};
use crate::testing::FakeResolver;
use crate::transport::{Outbound, Transport as _};

#[tokio::test]
async fn should_refuse_mixed_dns_after_the_public_http_policy_check() {
    let mut policy = policy();
    policy.network_policy.http_egress = HttpEgress::Public;
    let allowed = EgressPolicy::new(&policy)
        .allow(Draft {
            method: "GET".into(),
            url: "https://mixed.example.invalid:9/log".into(),
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
        })
        .expect("public scope allows the unlisted hostname before resolution");
    let resolver = FakeResolver::answering(&[(
        "mixed.example.invalid",
        &[
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ],
    )]);
    let network = Network::build(
        reqwest::Client::builder(),
        Arc::new(resolver),
        super::super::Limits::DEFAULT,
    )
    .expect("valid client");
    let refused = network
        .send(Outbound {
            method: allowed.method,
            url: allowed.url,
            headers: HeaderMap::new(),
            body: None,
            discard_redirect_body: false,
        })
        .await
        .expect_err("a mixed public/private answer is refused whole");
    assert_eq!(refused.refusal(), Some(crate::Refusal::EgressDenied));
}
