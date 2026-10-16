//! Production admission, with pointed fixture selection like the fleet lease suite.

mod capacity;
mod failure;
mod fixture;
mod funnel;
mod identity;
mod obligations;
mod replacement;
mod stop;

use super::{
    fixture::{Fixture, now},
    mint,
};
use fixture::{claim, funded, queue};
