//! What the lease verb does with a fleet's REPOSITORY binding.
//!
//! Split from [`super::cases`] along the seam the verb has: that module proves
//! what each approval VERDICT becomes, and this proves what a binding becomes
//! once admission is already past. The two never share a fixture — a case here
//! raises no event gate at all, because an event gate would stop the pass
//! before the binding is ever read.
//!
//! # The claim, and why it takes three cases
//!
//! `Plane::repair_branch` answers two ways the delivery can tell apart, and a
//! branch that is READ rather than decided is what needs both: an
//! implementation returning a fixed `None` passes the refusal case, and one
//! returning a fixed branch passes the delivery case. Neither survives the
//! pair, because the branch is derived from a gate identifier minted per run.
//!
//! The read binding is the third case and a different claim — that a read
//! reach is never put through the WRITE rules. Its short-circuit inside
//! `repair_branch` is not what it grades: removing that early return changes
//! nothing observable, because a read binding's egress build never consults
//! the branch it would then have fetched. The guard saves a query, and this
//! suite says so rather than claiming a proof it does not have.
//!
//! The refusal is the one with no prior grader: `BINDING_UNENFORCEABLE` is
//! written at exactly one place in this daemon and nothing read it back. It is
//! also the arm that must NOT be a park — a fleet author's mistake never
//! becomes different on the next poll, so an event that retried it would
//! redeliver forever against a config nobody is going to fix by waiting.

use super::seed::{seed_gate, seed_provider_resolution, seed_write_gate};
use super::*;

use afd_core::event::label;
use afd_gate::policy::repair;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_write_binding_with_no_approval_ends_the_event_as_unenforceable() {
    // `repair_branch` finds no approved write gate, so the egress build has no
    // branch to lock its rules to and refuses the binding outright. The event
    // must END: the config cannot be enforced, and nothing about the next poll
    // changes that, so parking it would be the redelivery loop this milestone
    // exists to close.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, WRITE_BOUND_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    seed_gate(&fixtures, &seeded, STATUS_APPROVED).await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        answer.contains(NO_LEASE),
        "an unenforceable binding issued a lease: {answer}"
    );
    let (status, failure) = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id)
        .await
        .expect("an unenforceable binding opens a narrative row");
    assert_eq!(
        status, STATUS_GATE_BLOCKED,
        "the row must be terminal; a parked binding redelivers forever"
    );
    assert_eq!(
        failure,
        label::BINDING_UNENFORCEABLE,
        "an unenforceable binding must not be recorded as some other refusal"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_approved_write_binding_locks_its_egress_to_that_gates_branch() {
    // The mirror of the refusal above, and the half that makes the pair a test
    // rather than a coincidence: the branch is derived from the GATE's
    // identifier, which is freshly minted per run, so no fixed string can
    // satisfy this assertion. A `repair_branch` hardcoded to `None` fails here
    // while still passing the case above.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, WRITE_BOUND_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    seed_gate(&fixtures, &seeded, STATUS_APPROVED).await;
    let gate = seed_write_gate(&fixtures, &seeded, STATED_WRITE_BINDING).await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    let branch = repair::branch_for(&gate);
    assert!(
        answer.contains(&branch),
        "the delivery locked a branch that is not this gate's {branch}: {answer}"
    );
    assert!(
        !answer.contains(NO_LEASE),
        "an approved write binding issued no lease: {answer}"
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        Some((STATUS_RECEIVED.to_owned(), String::new())),
        "a delivered event has not failed"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_read_binding_is_delivered_without_asking_for_an_approval() {
    // A read reach needs no branch, so it must be delivered with no approval
    // seeded at all — the same fixture the first case refuses, differing in
    // one word of config. What this kills is a build that put a read binding
    // through `write::rules`: that binding has no repair branch and never
    // will, so it would refuse every read-bound fleet in the deployment.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, READ_BOUND_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        !answer.contains(NO_LEASE),
        "a read binding was refused a lease: {answer}"
    );
    assert!(
        !answer.contains(repair::PREFIX),
        "a read binding was given a repair branch it never asked for: {answer}"
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        Some((STATUS_RECEIVED.to_owned(), String::new())),
        "a delivered event has not failed"
    );

    fixtures.cleanup().await;
}
