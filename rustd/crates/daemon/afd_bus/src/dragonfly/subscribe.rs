use std::sync::Arc;

use afd_dragonfly::hub::Received as Native;

use crate::{Message, Received, Subject, Subscription};

pub(crate) fn open(subscription: afd_dragonfly::Subscription, subject: Subject) -> Subscription {
    Subscription::new(futures_util::stream::unfold(
        Some((subscription, subject)),
        async move |state| {
            let (mut subscription, subject) = state?;
            match subscription.recv().await {
                Ok(native) => {
                    let received = match native {
                        Native::Message(message) => Received::Message(Arc::new(Message {
                            subject: subject.clone(),
                            payload: Arc::clone(&message.payload),
                        })),
                        Native::Lagged(count) => Received::Lagged(count),
                        Native::Gap => Received::Gap,
                    };
                    Some((Ok(received), Some((subscription, subject))))
                }
                Err(error) => Some((Err(error.into()), None)),
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, sync::Arc};

    use afd_dragonfly::SubscriptionHub;

    use crate::{Received, Result, Subject};

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;
    const FLEET: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bb011";

    #[tokio::test]
    async fn every_bus_subscriber_observes_a_restored_subscription_as_a_gap() -> TestResult {
        let (hub, server) = SubscriptionHub::detached();
        let subject = Subject::FleetTail(af_core::id::Uuid7::parse(FLEET)?);
        let channel = super::super::channel(&subject);
        let mut first = super::open(hub.subscribe(&channel), subject.clone());
        let mut second = super::open(hub.subscribe(&channel), subject);
        server.confirm(&channel);
        server.confirm(&channel);
        assert_eq!(first.recv().await?, Received::Gap);
        assert_eq!(second.recv().await?, Received::Gap);
        Ok(())
    }

    #[tokio::test]
    async fn dropping_bus_subscriptions_releases_native_interest_and_payloads_are_shared()
    -> TestResult {
        let (hub, server) = SubscriptionHub::detached();
        let subject = Subject::FleetTail(af_core::id::Uuid7::parse(FLEET)?);
        let channel = super::super::channel(&subject);
        let mut first = super::open(hub.subscribe(&channel), subject.clone());
        let mut second = super::open(hub.subscribe(&channel), subject.clone());
        assert_eq!(hub.readers(&channel), 2);
        server.publish(&channel, "shared signal");
        let (Received::Message(one), Received::Message(two)) =
            (first.recv().await?, second.recv().await?)
        else {
            return Err(std::io::Error::other("message was not delivered").into());
        };
        assert_eq!(one.subject, subject);
        assert_eq!(one.payload.as_str(), "shared signal");
        assert!(
            Arc::ptr_eq(&one.payload, &two.payload),
            "no payload copy per viewer"
        );
        drop(first);
        assert_eq!(hub.readers(&channel), 1);
        drop(second);
        assert_eq!(hub.readers(&channel), 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_slow_bus_reader_is_told_how_many_native_messages_it_lost() -> TestResult {
        let (hub, server) = SubscriptionHub::detached();
        let subject = Subject::FleetTail(af_core::id::Uuid7::parse(FLEET)?);
        let channel = super::super::channel(&subject);
        let mut subscription = super::open(hub.subscribe(&channel), subject);
        for _ in 0..513 {
            server.publish(&channel, "signal");
        }
        assert!(matches!(subscription.recv().await?, Received::Lagged(257)));
        assert!(matches!(subscription.recv().await?, Received::Message(_)));
        Ok(())
    }

    #[tokio::test]
    async fn a_closed_native_hub_retains_its_original_cause() -> TestResult {
        let (hub, _server) = SubscriptionHub::detached();
        let subject = Subject::FleetTail(af_core::id::Uuid7::parse(FLEET)?);
        let channel = super::super::channel(&subject);
        let mut subscription = super::open(hub.subscribe(&channel), subject);
        hub.shutdown();
        let Err(error) = subscription.recv().await else {
            return Err(std::io::Error::other("closed hub delivered a message").into());
        };
        assert!(
            error
                .source()
                .is_some_and(<dyn std::error::Error>::is::<afd_dragonfly::Error>)
        );
        let Err(_) = subscription.recv().await else {
            return Err(std::io::Error::other("ended subscription delivered a message").into());
        };
        Ok(())
    }
}
