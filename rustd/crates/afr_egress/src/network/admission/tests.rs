//! Public HTTP admission still crosses the production resolver's address guard.

#![expect(clippy::expect_used, reason = "test preconditions fail by panicking")]
#![expect(
    clippy::disallowed_methods,
    reason = "the test passes a raw client builder to the production address guard"
)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use afd_wire::policy::HttpEgress;
use reqwest::header::HeaderMap;

use crate::admission::{Admission, Draft, Placement};
use crate::fixture::policy;
use crate::network::Network;
use crate::testing::FakeResolver;
use crate::transport::{Outbound, Transport as _};

#[tokio::test]
async fn should_refuse_mixed_dns_after_public_http_admission() {
    let mut policy = policy(true);
    policy.network_policy.http_egress = HttpEgress::Public;
    let admitted = Admission::new(&policy)
        .admit(Draft {
            method: "GET".into(),
            url: "https://mixed.example.invalid:9/log".into(),
            headers: Vec::new(),
            body: None,
            placement: Placement::Nowhere,
        })
        .expect("public scope admits the unlisted hostname before resolution");
    let resolver = FakeResolver::answering(&[(
        "mixed.example.invalid",
        &[
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        ],
    )]);
    let network =
        Network::build(reqwest::Client::builder(), Arc::new(resolver)).expect("valid client");
    let refused = network
        .send(Outbound {
            method: admitted.method,
            url: admitted.url,
            headers: HeaderMap::new(),
            body: None,
            discard_redirect_body: false,
        })
        .await
        .expect_err("a mixed public/private answer is refused whole");
    assert_eq!(refused.refusal(), Some(crate::Refusal::AddressNotAllowed));
}
