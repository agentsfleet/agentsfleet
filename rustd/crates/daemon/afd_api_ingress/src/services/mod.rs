//! What a signed-delivery handler acts through: [`IngressPlane`], and the
//! signup seams only this plane reaches.
//!
//! The seams another plane also reaches live in [`afd_http::services`]; the
//! private import below lets a handler name them beside these as
//! `crate::services::…`.

mod plane;
mod signup;

pub use self::plane::IngressPlane;
pub use self::signup::{IdentityWebhookSecret, SignupMetadata, Signups};

pub(crate) use afd_http::services::{
    FleetSchedules, WebhookIngress, WorkspaceApprovals, WorkspaceConnectors,
};
