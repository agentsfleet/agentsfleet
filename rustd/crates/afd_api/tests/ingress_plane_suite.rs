//! Signed-ingress HTTP regression suite.
//!
//! The webhook, connector-events and scheduled-fire surfaces: everything the
//! sender proves with a signature over the body rather than with a bearer.

mod harness;

#[path = "app_ingress_route.rs"]
mod app_ingress_route;
#[path = "connector_events_route.rs"]
mod connector_events_route;
mod identity_signup_route;
#[path = "ingress_plane_ownership.rs"]
mod ingress_plane_ownership;
#[path = "integration_app_ingress_live.rs"]
mod integration_app_ingress_live;
#[path = "integration_approval_webhook.rs"]
mod integration_approval_webhook;
#[path = "integration_connector_events.rs"]
mod integration_connector_events;
#[path = "integration_ingress_live.rs"]
mod integration_ingress_live;
#[path = "webhook_approval_wall.rs"]
mod webhook_approval_wall;
#[path = "webhook_fleet_route.rs"]
mod webhook_fleet_route;
#[path = "webhook_qstash_route.rs"]
mod webhook_qstash_route;
#[path = "webhook_receive_route.rs"]
mod webhook_receive_route;
#[path = "webhook_svix_route.rs"]
mod webhook_svix_route;
#[path = "webhook_wall_refusals.rs"]
mod webhook_wall_refusals;
