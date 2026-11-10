//! Exercise the object boundary, typed routing, payload sharing and shutdown.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use af_core::{error_code::INTERNAL_OPERATION_FAILED, id::Uuid7, test_util::trace::Capture};
use async_trait::async_trait;

use crate::{Bus, Handle, Received, Subject, Subscription, error::ErrorKind, test_util};

type Result<T = ()> = crate::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::test]
async fn shared_handles_preserve_subjects_payload_ownership_and_last_reader_drop() -> Result {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::FleetTail(Uuid7::parse("0195b4ba-8d3a-7000-8000-000000000001")?);
    let mut first = bus.subscribe(&subject).await?;
    let mut second = bus.clone().subscribe(&subject).await?;
    assert_eq!(server.readers(&subject), 2);
    server.publish(&subject, "fixture-message");
    let Received::Message(left) = first.recv().await? else {
        return Err("message missing".into());
    };
    let Received::Message(right) = second.recv().await? else {
        return Err("message missing".into());
    };
    assert_eq!(left.subject, subject);
    assert_eq!(left.payload.as_str(), "fixture-message");
    assert!(Arc::ptr_eq(&left.payload, &right.payload));
    drop(first);
    assert_eq!(server.readers(&subject), 1);
    drop(second);
    assert_eq!(server.readers(&subject), 0);
    Ok(())
}

#[tokio::test]
async fn shutdown_closes_existing_readers_and_rejects_every_new_operation() -> Result {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::Wake(0);
    let mut subscription = bus.subscribe(&subject).await?;
    bus.clone().shutdown().await?;
    bus.shutdown().await?;
    assert_eq!(server.readers(&subject), 0);
    let Err(_) = subscription.recv().await else {
        return Err("shutdown reader delivered a message".into());
    };
    let Err(error) = bus.subscribe(&subject).await else {
        return Err("closed bus resubscribed".into());
    };
    assert_eq!(error.reason(), "closed");
    let Err(error) = bus.publish(&subject, "fixture-message").await else {
        return Err("closed bus published".into());
    };
    assert_eq!(error.reason(), "closed");
    let Err(error) = bus.publish_batch(&subject, &[]).await else {
        return Err("closed bus accepted a batch".into());
    };
    assert_eq!(error.reason(), "closed");
    Ok(())
}

#[tokio::test]
async fn reconnect_and_lag_cross_the_object_boundary_without_backend_types() -> Result {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::DraftEvents(Uuid7::parse("0195b4ba-8d3a-7000-8000-000000000001")?);
    let mut first = bus.subscribe(&subject).await?;
    let mut second = bus.subscribe(&subject).await?;
    server.gap(&subject);
    assert_eq!(first.recv().await?, Received::Gap);
    assert_eq!(second.recv().await?, Received::Gap);
    for _ in 0..513 {
        server.publish(&subject, "fixture-message");
    }
    assert_eq!(first.recv().await?, Received::Lagged(257));
    assert_eq!(second.recv().await?, Received::Lagged(257));
    bus.publish_batch(&subject, &[]).await?;
    bus.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn an_adapter_with_no_connection_to_lose_reports_connected() -> Result {
    // The detached adapter keeps the trait's default, which `/readyz` reads
    // through the handle without knowing which adapter booted.
    let (bus, _server) = test_util::detached()?;
    assert!(bus.connected().await);
    Ok(())
}

/// The payload [`Defaults`] refuses.
const REFUSED: &str = "refused";

/// An adapter that keeps every default the trait offers, recording each
/// payload it publishes and refusing [`REFUSED`].
#[derive(Debug)]
struct Defaults {
    published: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Bus for Defaults {
    async fn publish(&self, _subject: &Subject, payload: &str) -> crate::Result<()> {
        if payload == REFUSED {
            return Err(ErrorKind::NatsDisconnected.into());
        }
        self.published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(payload.to_owned());
        Ok(())
    }

    async fn subscribe(&self, _subject: &Subject) -> crate::Result<Subscription> {
        Err(ErrorKind::Closed.into())
    }

    async fn shutdown(&self) -> crate::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn the_default_batch_publishes_in_order_and_stops_at_the_first_failure() -> Result {
    let published = Arc::new(Mutex::new(Vec::new()));
    let bus = Handle::new(Defaults {
        published: Arc::clone(&published),
    });
    let subject = Subject::OutboundWake;
    let read = || {
        published
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    };

    bus.publish_batch(&subject, &["first".to_owned(), "second".to_owned()])
        .await?;
    assert_eq!(read(), ["first", "second"]);
    let batch = ["third", REFUSED, "never"].map(str::to_owned);
    let Err(error) = bus.publish_batch(&subject, &batch).await else {
        return Err("a refused payload fails its batch".into());
    };
    assert_eq!(error.code(), INTERNAL_OPERATION_FAILED);
    assert_eq!(error.reason(), "backend");
    assert_eq!(
        read(),
        ["first", "second", "third"],
        "the prefix before the failure went out, nothing after it"
    );
    Ok(())
}

#[tokio::test]
async fn the_default_reports_an_adapter_with_no_connection_to_lose_as_connected() {
    let bus = Handle::new(Defaults {
        published: Arc::default(),
    });
    assert!(bus.connected().await);
}

#[tokio::test]
async fn a_frame_json_cannot_spell_is_refused_before_any_reader_sees_it() -> Result {
    let (bus, server) = test_util::detached()?;
    let subject = Subject::Wake(2);
    let mut reader = bus.subscribe(&subject).await?;
    // A JSON object's keys are strings, and a pair has no string spelling.
    let frame = BTreeMap::from([((1_u8, 2_u8), "unencodable")]);

    let Err(error) = bus.publish_frame(&subject, &frame).await else {
        return Err("a frame JSON cannot encode is refused".into());
    };
    assert_eq!(error.reason(), "serialization");
    assert_eq!(error.code(), INTERNAL_OPERATION_FAILED);
    assert!(!error.is_closed(), "the bus itself is still open");

    server.publish(&subject, "after");
    let Received::Message(next) = reader.recv().await? else {
        return Err("message missing".into());
    };
    assert_eq!(
        next.payload.as_str(),
        "after",
        "nothing reached the reader first"
    );
    Ok(())
}

#[tokio::test]
async fn a_subscription_debugs_without_its_stream() -> Result {
    let (bus, _server) = test_util::detached()?;
    let subscription = bus.subscribe(&Subject::Wake(1)).await?;
    assert_eq!(format!("{subscription:?}"), "Subscription { .. }");
    Ok(())
}

#[test]
fn a_dropped_notification_names_its_subject_kind_and_only_a_tail_names_its_fleet() -> Result {
    const FLEET: &str = "0195b4ba-8d3a-7000-8000-000000000001";
    let fleet = Uuid7::parse(FLEET)?;
    let capture = Capture::install();

    for subject in [
        Subject::FleetTail(fleet.clone()),
        Subject::DraftEvents(fleet.clone()),
        Subject::Wake(3),
        Subject::RunnerCommand(fleet),
        Subject::OutboundWake,
    ] {
        crate::observe_notification(&subject, Err(ErrorKind::NatsDisconnected.into()));
    }
    crate::observe_notification(&Subject::Wake(4), Ok(()));

    let events = capture.events();
    assert_eq!(
        events
            .iter()
            .map(|event| event.field("subject_kind"))
            .collect::<Vec<_>>(),
        [
            Some("fleet_tail"),
            Some("draft_events"),
            Some("wake"),
            Some("runner_command"),
            Some("outbound_wake"),
        ],
        "one line per failed notification, none for a delivered one"
    );
    assert_eq!(
        events
            .iter()
            .map(|event| event.field("fleet_id"))
            .collect::<Vec<_>>(),
        [Some(FLEET), None, None, None, None]
    );
    for event in &events {
        assert_eq!(event.level, tracing::Level::WARN);
        assert_eq!(event.field("event"), Some("tail_frame_dropped"));
        assert_eq!(
            event.field("error_code"),
            Some(INTERNAL_OPERATION_FAILED.as_str())
        );
        assert_eq!(event.field("reason"), Some("backend"));
    }
    Ok(())
}
