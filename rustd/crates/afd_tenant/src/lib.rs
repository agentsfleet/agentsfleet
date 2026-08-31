//! The tenant plane: what a person or an organisation manages for itself.
//!
//! Api-keys, command-line credentials, the device-flow login surface, and the
//! workspace-ownership resolver every guarded route is checked against.
//!
//! # Why this is its own crate
//!
//! It was carved out of `afd_fleet`, which had grown to half the workspace —
//! 28,500 lines, 4.8 times the next crate — so an edit to a 400-line module
//! rebuilt all of it and nothing else could compile until it finished.
//!
//! The cut is where the dependency graph actually separates. The runner plane's
//! modules are mutually circular (`lease → credential → lease`,
//! `provider ↔ vault ↔ secrets`) and cannot be split by moving files at all;
//! these four had exactly one edge out of the group — `Minted`, which moved
//! down to `afd_auth` beside the classifier that supplies its marker — and none
//! between themselves and the rest. What is left here is acyclic and depends
//! only on the value crates below it.
//!
//! # What it does not contain
//!
//! No routing, no HTTP, no extractor. Who MAY call a verb is decided at the
//! edge, in `afd_api`, by a layer mounted from the route's own template; this
//! crate answers what a verb DOES once that decision is made. The split is the
//! reason `workspace::Workspaces::authorize` is here and the middleware calling
//! it is not.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod apikey;
pub mod cli_credential;
pub mod error;
pub mod models;
pub mod preference;
pub mod provider;
pub mod session;
pub mod signup;
pub mod sql;
pub mod workspace;

pub use self::error::{Error, Result};
