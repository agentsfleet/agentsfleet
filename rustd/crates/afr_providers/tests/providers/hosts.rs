//! Which host a turn reaches: the one its route names, and no other. A
//! redirect is never followed, so the key rides to one host; a `custom:`
//! endpoint whose name resolves inside the runner's own network is refused by
//! the client's resolver, before a connection opens.

use afd_wire::policy::CUSTOM_PROVIDER_PREFIX;
use afd_wire::report::{FailureClass, ResultOutcome};

use super::ANSWER;
use super::support::wires::Wire;
use super::support::{Fake, Reply, engine, lease, run};

#[tokio::test]
async fn a_redirect_is_never_followed_so_the_key_reaches_one_host() {
    let mut fake = Fake::serve(vec![Reply::Redirect("/elsewhere".to_owned())]).await;
    let leased = lease(&Wire::Messages.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a redirected turn is no answer");
    };
    assert!(failure.detail.contains("307"), "{}", failure.detail);
    let seen = fake.seen();
    assert_eq!(seen.len(), 1, "the redirect's target was never asked");
    assert_eq!(seen[0].path, Wire::Messages.path());
    assert!(
        fake.connections() >= 1,
        "the one request the fake read came over a connection it counted"
    );
}

// A loopback literal is refused at admission, where no resolver runs; a name
// only shows what it is on the lookup, so the client's resolver refuses it
// there, and what the run reports is the connection it never got.
#[tokio::test]
async fn should_refuse_a_custom_endpoint_whose_name_resolves_to_loopback() {
    let mut fake = Fake::serve(vec![Wire::Chat.answer(ANSWER)]).await;
    let Some((_, port)) = fake.base.rsplit_once(':') else {
        panic!("the fake's base names its port: {}", fake.base);
    };
    let endpoint = format!("{CUSTOM_PROVIDER_PREFIX}https://localhost:{port}/v1");
    let leased = lease(&endpoint, &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("an endpoint the resolver refuses is no answer");
    };
    assert_eq!(
        failure.class,
        Some(FailureClass::TransportLoss),
        "{}",
        failure.detail
    );
    assert_eq!(fake.connections(), 0, "refused before anything connected");
    assert!(fake.seen().is_empty());
}
