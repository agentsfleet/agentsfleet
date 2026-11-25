//! Every `af_fleet_runtime` test file, in one test binary.
//!
//! cargo runs test BINARIES serially and the tests inside one in parallel, so
//! each extra binary is a serial stretch re-paying process start.

mod error_surface;
