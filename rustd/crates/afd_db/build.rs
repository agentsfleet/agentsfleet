//! Lists the `schema/*.sql` files, so a test can hold the migration list to them.
//!
//! `afd_db::migration::MIGRATIONS` stays hand-written (its module doc says why).
//! What a hand-written list can do is forget a file, and a forgotten file never
//! runs. This script writes the directory's `.sql` names, sorted, into
//! `$OUT_DIR/schema_files.rs`; `tests/migrations.rs` includes that slice and
//! fails on any name the list does not carry.
//!
//! A directory that cannot be read writes an empty slice rather than failing
//! the build: the production binary does not need the listing, and the test
//! refuses an empty one as a broken scan.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The schema directory, from this crate's manifest.
const SCHEMA: &str = "../../../schema";

/// The file `tests/migrations.rs` includes.
const LISTING: &str = "schema_files.rs";

#[expect(
    clippy::disallowed_methods,
    reason = "a build script has no build-dependency on afd_core, and OUT_DIR is cargo's, not a knob"
)]
fn out_dir() -> Option<PathBuf> {
    std::env::var_os("OUT_DIR").map(PathBuf::from)
}

/// Every `.sql` filename in `schema/`, sorted; empty when the directory is unreadable.
fn schema_files() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(SCHEMA) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "sql"))
        .filter_map(|path| path.file_name()?.to_str().map(str::to_owned))
        .collect();
    names.sort_unstable();
    names
}

fn main() -> std::io::Result<()> {
    // A directory path makes cargo rerun on any file added, removed or edited.
    println!("cargo:rerun-if-changed={SCHEMA}");
    println!("cargo:rerun-if-changed=build.rs");

    let Some(out) = out_dir() else {
        return Err(std::io::Error::other("cargo did not set OUT_DIR"));
    };
    let mut listing = String::from("&[\n");
    for name in schema_files() {
        // `{:?}` writes a Rust string literal, escapes included.
        let _ = writeln!(listing, "    {name:?},");
    }
    listing.push_str("]\n");
    std::fs::write(Path::new(&out).join(LISTING), listing)
}
