use std::sync::Arc;

use af_core::id::Uuid7;
use async_nats::HeaderMap;
use bytes::Bytes;

use super::{Channels, Delivery, GAP_HEADER, GAP_VALUE, classify};
use crate::{Message, Received, Subject};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const FLEET: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";
const PHYSICAL: &str = "af.fleet.0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011.tail";

fn subject() -> Result<Subject, Box<dyn std::error::Error + Send + Sync>> {
    Ok(Subject::FleetTail(Uuid7::parse(FLEET)?))
}

fn native(payload: &'static [u8], headers: Option<HeaderMap>) -> async_nats::Message {
    async_nats::Message {
        subject: PHYSICAL.into(),
        reply: None,
        payload: Bytes::from_static(payload),
        headers,
        status: None,
        description: None,
        length: payload.len(),
    }
}

fn marked() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(GAP_HEADER, GAP_VALUE);
    headers
}

#[test]
fn marker_needs_the_header_not_the_payload() -> TestResult {
    let subject = subject()?;
    let Some(Delivery::Message(message)) = classify(&subject, &native(b"", None)) else {
        return Err("an empty body with no header is a message".into());
    };
    assert_eq!(message.payload.as_str(), "");
    assert!(matches!(
        classify(&subject, &native(b"", Some(marked()))),
        Some(Delivery::Gap)
    ));
    assert!(
        matches!(
            classify(&subject, &native(b"{\"kind\":\"x\"}", Some(marked()))),
            Some(Delivery::Gap)
        ),
        "the header wins over any payload"
    );
    let mut other = HeaderMap::new();
    other.insert(GAP_HEADER, "0");
    assert!(matches!(
        classify(&subject, &native(b"", Some(other))),
        Some(Delivery::Message(_))
    ));
    Ok(())
}

#[test]
fn a_payload_that_is_not_utf8_is_dropped() -> TestResult {
    assert!(classify(&subject()?, &native(b"\xff\xfe", None)).is_none());
    Ok(())
}

#[tokio::test]
async fn readers_share_one_channel_and_one_payload() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let first = channels.open(PHYSICAL);
    let (sender, _) = first.pump.ok_or("the first reader opens the channel")?;
    let second = channels.open(PHYSICAL);
    assert!(second.pump.is_none(), "the second reader joins");
    assert_eq!(channels.readers(PHYSICAL), 2);
    let message = Arc::new(Message {
        subject: subject()?,
        payload: Arc::new("shared".to_owned()),
    });
    sender.send(Delivery::Message(Arc::clone(&message)))?;
    let (mut one, mut two) = (first.reader, second.reader);
    let (Received::Message(a), Received::Message(b)) = (one.recv().await?, two.recv().await?)
    else {
        return Err("both readers receive the message".into());
    };
    assert!(Arc::ptr_eq(&a, &b), "no copy per reader");
    Ok(())
}

#[tokio::test]
async fn the_last_reader_stops_the_pump() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let opened = channels.open(PHYSICAL);
    let (_sender, generation) = opened.pump.ok_or("opened")?;
    let pump = tokio::spawn(std::future::pending::<()>());
    channels.started(PHYSICAL, generation, pump.abort_handle());
    let joined = channels.open(PHYSICAL).reader;
    drop(opened.reader);
    assert_eq!(channels.readers(PHYSICAL), 1);
    assert!(!pump.is_finished(), "a reader remains");
    drop(joined);
    assert_eq!(channels.readers(PHYSICAL), 0);
    let Err(stopped) = pump.await else {
        return Err("the pump must be aborted".into());
    };
    assert!(stopped.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn a_pump_started_after_its_readers_left_stops_at_once() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let opened = channels.open(PHYSICAL);
    let (_sender, generation) = opened.pump.ok_or("opened")?;
    drop(opened.reader);
    let pump = tokio::spawn(std::future::pending::<()>());
    channels.started(PHYSICAL, generation, pump.abort_handle());
    let Err(stopped) = pump.await else {
        return Err("a late pump must be aborted".into());
    };
    assert!(stopped.is_cancelled());
    Ok(())
}

#[tokio::test]
async fn a_gap_reaches_every_reader_of_every_subject() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let mut readers = vec![
        channels.open(PHYSICAL).reader,
        channels.open(PHYSICAL).reader,
        channels.open("af.wake.3").reader,
    ];
    assert_eq!(channels.gap_all(), 3);
    for reader in &mut readers {
        assert_eq!(reader.recv().await?, Received::Gap);
    }
    Ok(())
}

#[tokio::test]
async fn a_slow_reader_is_told_how_many_it_lost() -> TestResult {
    let channels = Arc::new(Channels::new(4));
    let opened = channels.open(PHYSICAL);
    let (sender, _) = opened.pump.ok_or("opened")?;
    let mut reader = opened.reader;
    for _ in 0..6 {
        sender.send(Delivery::Gap)?;
    }
    assert_eq!(reader.recv().await?, Received::Lagged(2));
    assert_eq!(reader.recv().await?, Received::Gap);
    Ok(())
}

#[tokio::test]
async fn an_ended_pump_closes_its_readers_and_a_new_reader_reopens() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let opened = channels.open(PHYSICAL);
    let (sender, generation) = opened.pump.ok_or("opened")?;
    let mut subscription = opened.reader.into_subscription();
    channels.ended(PHYSICAL, generation);
    drop(sender);
    let Err(error) = subscription.recv().await else {
        return Err("an ended channel must close".into());
    };
    assert_eq!(error.reason(), "closed");
    let Err(_) = subscription.recv().await else {
        return Err("a closed subscription stays closed".into());
    };
    let reopened = channels.open(PHYSICAL);
    let (_, next) = reopened.pump.ok_or("a fresh channel needs a fresh pump")?;
    assert_ne!(next, generation);
    channels.ended(PHYSICAL, generation);
    assert_eq!(
        channels.readers(PHYSICAL),
        1,
        "a stale end leaves the new channel"
    );
    Ok(())
}

#[test]
fn a_stale_reader_leaves_the_reopened_channel_alone() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let first = channels.open(PHYSICAL);
    let (_, generation) = first.pump.ok_or("opened")?;
    channels.ended(PHYSICAL, generation);
    let second = channels.open(PHYSICAL);
    assert!(
        second.pump.is_some(),
        "an ended subject reopens with a new pump"
    );
    drop(first.reader);
    assert_eq!(
        channels.readers(PHYSICAL),
        1,
        "the old reader's release is ignored"
    );
    drop(second.reader);
    assert_eq!(channels.readers(PHYSICAL), 0);
    Ok(())
}

#[tokio::test]
async fn close_ends_every_channel() -> TestResult {
    let channels = Arc::new(Channels::new(8));
    let opened = channels.open(PHYSICAL);
    let (sender, generation) = opened.pump.ok_or("opened")?;
    let pump = tokio::spawn(std::future::pending::<()>());
    channels.started(PHYSICAL, generation, pump.abort_handle());
    channels.close();
    drop(sender);
    let mut reader = opened.reader;
    let Err(_) = reader.recv().await else {
        return Err("a closed hub ends its readers".into());
    };
    let Err(stopped) = pump.await else {
        return Err("close aborts every pump".into());
    };
    assert!(stopped.is_cancelled());
    assert_eq!(channels.readers(PHYSICAL), 0);
    Ok(())
}

#[test]
fn concurrent_openers_make_exactly_one_pump() -> TestResult {
    const OPENERS: usize = 16;
    let channels = Arc::new(Channels::new(8));
    let barrier = Arc::new(std::sync::Barrier::new(OPENERS));
    let mut threads = Vec::new();
    for _ in 0..OPENERS {
        let channels = Arc::clone(&channels);
        let barrier = Arc::clone(&barrier);
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            let opened = channels.open(PHYSICAL);
            (opened.pump.is_some(), opened.reader)
        }));
    }
    let mut pumps = 0;
    let mut readers = Vec::new();
    for thread in threads {
        let (pump, reader) = thread
            .join()
            .map_err(|panic| format!("an opener panicked: {panic:?}"))?;
        pumps += usize::from(pump);
        readers.push(reader);
    }
    assert_eq!(pumps, 1, "one native subscription per subject");
    assert_eq!(channels.readers(PHYSICAL), OPENERS);
    drop(readers);
    assert_eq!(channels.readers(PHYSICAL), 0);
    Ok(())
}
