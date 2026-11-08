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
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned());
    let client = Dragonfly::unreachable(&config)?;
    let bus = dragonfly::attached(client, Some(hub.clone()));
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
