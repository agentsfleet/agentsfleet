//! Production admission, with pointed fixture selection like the fleet lease suite.

mod capacity;
mod context;
mod failure;
mod fixture;
mod funnel;
mod holds;
mod identity;
mod obligations;
mod offered;
mod platform;
mod replacement;
mod stop;

use super::{
    fixture::{Fixture, now},
    mint,
};
use fixture::{claim, funded, queue};
