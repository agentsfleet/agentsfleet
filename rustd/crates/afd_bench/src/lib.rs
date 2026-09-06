//! Measuring what the paths a fleet's life runs through actually sustain.
//!
//! ```text
//!   profile ──► caps ──► parameters refused BEFORE a connection opens
//!      │                 (a lane cannot be pointed somewhere dangerous
//!      │                  by a stray command-line number)
//!      │
//!      ├──► target ──► rig datastores, or a deployed address
//!      │
//!      └──► fixture ──► every created object carries the run prefix,
//!                       and the sweep that removes them reports its count
//! ```
//!
//! # Why a profile decides, and not each lane
//!
//! Four lanes ask four different questions, but they share one hazard: they
//! generate load, and load pointed at the wrong datastore is an incident. So
//! scale, target and blast radius are resolved ONCE, from the profile, before
//! a lane opens anything. A lane reads its ceiling; it never sets one.
//!
//! # These lanes measure and change nothing
//!
//! Every lane drives the production types directly — `afd_fleet`'s lease path,
//! `afd_outbound`'s worker, `afd_events`' steer — rather than a copy of them,
//! because a reimplementation would measure the reimplementation. Nothing here
//! is reachable from the daemon, and no measurement is emitted as a metric:
//! results are files, so a synthetic run can never be mistaken for production
//! traffic on an operator's dashboard.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

pub mod datastores;
pub mod error;
pub mod fixture;
pub mod instrument;
pub mod lane;
pub mod profile;
pub mod report;

pub use datastores::Datastores;
pub use error::{Error, Result};
pub use fixture::{FixtureLedger, RunPrefix};
pub use instrument::{LeaseInstrument, PollCounters};
pub use profile::{Caps, Parameter, Profile, Target};
pub use report::{Lane, Latency, Report};
