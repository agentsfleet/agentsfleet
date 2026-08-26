#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "tests inspect canonical fixtures and thread results"
)]

use std::sync::{Arc, Barrier};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use super::LiveStreams;

const WORKSPACE_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb001";
const FLEET_ID: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb010";

fn fixture_id(raw: &str) -> Uuid7 {
    Uuid7::parse(raw).unwrap_or_else(|error| panic!("canonical fixture: {error}"))
}

#[test]
fn dropping_a_registration_removes_its_operator_row() {
    let streams = LiveStreams::new(3);
    let registration = streams
        .try_register(
            &fixture_id(WORKSPACE_ID),
            &fixture_id(FLEET_ID),
            UnixMillis::from_millis(12),
        )
        .expect("the first slot is available");
    assert_eq!(streams.overview().total, 1);
    drop(registration);
    assert_eq!(streams.overview().total, 0);
}

#[test]
fn capacity_claim_is_atomic_under_contention() {
    let streams = LiveStreams::new(1);
    let start = Arc::new(Barrier::new(3));
    let ready = Arc::new(Barrier::new(3));
    let release = Arc::new(Barrier::new(3));
    let contenders: Vec<_> = (0..2)
        .map(|_| {
            let streams = streams.clone();
            let start = Arc::clone(&start);
            let ready = Arc::clone(&ready);
            let release = Arc::clone(&release);
            std::thread::spawn(move || {
                start.wait();
                let registration = streams.try_register(
                    &fixture_id(WORKSPACE_ID),
                    &fixture_id(FLEET_ID),
                    UnixMillis::from_millis(12),
                );
                ready.wait();
                release.wait();
                registration.is_some()
            })
        })
        .collect();
    start.wait();
    ready.wait();
    assert_eq!(streams.overview().total, 1);
    release.wait();
    let admitted = contenders
        .into_iter()
        .map(|thread| thread.join().unwrap_or(false))
        .filter(|admitted| *admitted)
        .count();
    assert_eq!(admitted, 1);
}
