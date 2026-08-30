//! Signed-ingress HTTP regression suite.
//!
//! The webhook, connector-events and scheduled-fire surfaces: everything the
//! sender proves with a signature over the body rather than with a bearer.

mod harness;

#[path = "connector_events_route.rs"]
mod connector_events_route;
mod identity_signup_route;
#[path = "integration_approval_webhook.rs"]
mod integration_approval_webhook;
#[path = "webhook_approval_wall.rs"]
mod webhook_approval_wall;
#[path = "webhook_fleet_route.rs"]
mod webhook_fleet_route;
#[path = "webhook_wall_refusals.rs"]
mod webhook_wall_refusals;
