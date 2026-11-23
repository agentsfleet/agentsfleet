//! Re-asking a fleet stream's caller whether they may still read it.
//!
//! The ownership layer decided once, when the stream opened, and a tab stays
//! open for hours. An owner who removes a member expects the member's view to
//! stop, so the removal publishes a notice on the account after it commits, and
//! the stream asks the question again when one arrives, ending with
//! `access_revoked` when the answer turns. A gap or a lost subscription, which may
//! have hidden a notice, asks it too. No timer asks it. The workspace stream asks
//! the same question on the same notice (`wall.rs`), where it also re-reads the
//! fleet set; a single fleet has no set to refresh, so it carries only this.
//!
//! What an unanswered re-check means is the [`Watch`]'s, shared with the wall:
//! logged, and the stream ends (`revocable.rs`).

use std::sync::Arc;

use afd_auth::principal::Principal;
use afd_http::auth::Owned;
use afd_sse::{Frame, Link, Wake};
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};

use super::revocable::{Membership, Recheck, STREAM_FLEET, Turn, Verdict, Watch, until_revoked};
use crate::services::TenantPlane;

/// Everything a guarded stream carries between frames.
struct Guard<R> {
    frames: BoxStream<'static, Frame>,
    watch: Watch<R>,
    /// Each item says the account's members may have changed.
    notices: BoxStream<'static, Wake>,
}

/// `frames`, ended with `access_revoked` once `principal` loses the workspace.
pub(super) async fn guarded<D: TenantPlane>(
    frames: BoxStream<'static, Frame>,
    services: Arc<D>,
    principal: Principal,
    owned: Owned,
) -> BoxStream<'static, Frame> {
    let Owned { workspace, tenant } = owned;
    let link = services
        .live()
        .link(afd_bus::Subject::Members(tenant))
        .await;
    watched(
        frames,
        Membership::new(services, principal, workspace),
        notices(link),
    )
}

/// What `link` wakes on, as a stream, so a `select!` can hold it across turns. It opens
/// with one wake: access was proved before the link opened, so a removal published in
/// between is asked about here.
fn notices(link: Link) -> BoxStream<'static, Wake> {
    let following = stream::unfold(link, async |mut link| {
        let wake = link.changed().await;
        Some((wake, link))
    });
    stream::once(std::future::ready(Wake::Changed))
        .chain(following)
        .boxed()
}

/// The log event a fleet stream emits when the bus shut down under it.
const EVENT_BUS_CLOSED: &str = "fleet_stream_bus_closed";

/// `frames`, ended with `access_revoked` once `recheck` says no on a notice.
fn watched<R: Recheck>(
    frames: BoxStream<'static, Frame>,
    recheck: R,
    notices: BoxStream<'static, Wake>,
) -> BoxStream<'static, Frame> {
    let guard = Guard {
        frames,
        watch: Watch::new(recheck, STREAM_FLEET),
        notices,
    };
    until_revoked(guard, step)
}

/// The next frame, or the check that ends the stream.
async fn step<R: Recheck>(mut guard: Guard<R>) -> Option<Turn<Guard<R>>> {
    loop {
        tokio::select! {
            frame = guard.frames.next() => return frame.map(|frame| Turn::Frame(frame, guard)),
            notice = guard.notices.next() => match notice {
                Some(Wake::Changed) => match guard.watch.verdict().await {
                    Verdict::Admitted => {}
                    Verdict::Revoked => return Some(Turn::Revoked),
                    // An outage is not a revocation: the stream ends with no
                    // frame, and the reconnect is authorized at open.
                    Verdict::Unverified => return None,
                },
                // The bus shut down: end, so the client reconnects to a serving
                // replica, whose open check is fresh.
                Some(Wake::Closed) | None => {
                    tracing::debug!(event = EVENT_BUS_CLOSED);
                    return None;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests;
