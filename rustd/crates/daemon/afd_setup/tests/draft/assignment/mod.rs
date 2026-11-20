//! Production assignment, with pointed fixture selection like the fleet run suite.

mod assigned;
mod capacity;
mod context;
mod deliveries;
mod discovery;
mod failure;
mod fixture;
mod funnel;
mod history;
mod identity;
mod placement;
mod platform;
mod policy;
mod recovery;
mod rejections;
mod replacement;
mod revalidation;
mod skipping;
mod stop;
mod unreadable;

use super::{
    fixture::{Fixture, now},
    mint,
};
use fixture::{assign, funded, queue};
