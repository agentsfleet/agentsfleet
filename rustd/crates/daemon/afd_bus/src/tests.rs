//! Exercise the object boundary, typed routing, payload sharing and shutdown.

use std::sync::Arc;

use af_core::id::Uuid7;

use crate::{Received, Subject, test_util};

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
    server.confirm(&subject);
    server.confirm(&subject);
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
