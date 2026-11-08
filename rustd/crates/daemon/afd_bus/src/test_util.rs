//! The real adapter and native fan-out, driven at their server boundary.

use afd_dragonfly::{Dragonfly, DragonflyConfig, SubscriptionHub, config::DragonflyRole};

use crate::{Handle, Result, Subject, dragonfly};

/// Detached native server; no datastore or network socket is required.
#[derive(Debug)]
pub struct Server {
    native: afd_dragonfly::hub::DetachedServer,
    hub: SubscriptionHub,
}

/// Build the real subscription adapter over a detached server.
///
/// # Errors
/// Returns invalid local fixture configuration; publication still needs a live broker.
pub fn detached() -> Result<(Handle, Server)> {
    let (hub, native) = SubscriptionHub::detached();
    let bus = with_hub(hub.clone())?;
    Ok((bus, Server { native, hub }))
}

impl Server {
    /// Inject a server message through the production fan-out path.
    pub fn publish(&self, subject: &Subject, payload: &str) {
        self.native.publish(&dragonfly::channel(subject), payload);
    }

    /// A repeated server confirmation becomes a gap for every consumer.
    pub fn confirm(&self, subject: &Subject) {
        self.native.confirm(&dragonfly::channel(subject));
    }

    /// Counts actual native interests, including last-subscriber drop behavior.
    #[must_use]
    pub fn readers(&self, subject: &Subject) -> usize {
        self.hub.readers(&dragonfly::channel(subject))
    }
}

/// Bind the production adapter to a fixture's already-open native resources.
#[must_use]
pub fn attached(client: Dragonfly, hub: SubscriptionHub) -> Handle {
    dragonfly::attached(client, Some(hub))
}

/// Adapt a fixture's native subscription through the production conversion.
#[must_use]
pub fn subscription(native: afd_dragonfly::Subscription, subject: Subject) -> crate::Subscription {
    dragonfly::subscribe::open(native, subject)
}

/// Attach an existing native fixture hub without opening a publishing socket.
///
/// # Errors
/// Returns an invalid fixture client configuration.
pub fn with_hub(hub: SubscriptionHub) -> Result<Handle> {
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned());
    let client = Dragonfly::unreachable(&config)?;
    Ok(attached(client, hub))
}

/// Native address for a fixture exercising the server side of this adapter.
#[must_use]
pub fn physical_channel(subject: &Subject) -> String {
    dragonfly::channel(subject)
}

/// Decode a fixture's native fleet address before using the production conversion.
///
/// # Errors
/// Returns a noncanonical fixture identifier.
pub fn fleet_subscription(
    native: afd_dragonfly::Subscription,
) -> Result<crate::Subscription, af_core::error::Error> {
    let channel = native.channel();
    let fleet = channel
        .strip_prefix("fleet:")
        .and_then(|name| name.strip_suffix(":activity"))
        .unwrap_or(channel);
    let subject = Subject::FleetTail(af_core::id::Uuid7::parse(fleet)?);
    Ok(subscription(native, subject))
}
