//! What every sandbox answers alike: the grant it was built under, a
//! command's two streams and its exit code, and a tree that crosses in and
//! back out unchanged.

use std::io::Read as _;

use af_guest_wire::{Ending, Stream};
use afr_guest_client::Spawn;
use libtest_mimic::Failed;
use tar::{Archive, Builder, EntryType, Header};

use super::Context;
use super::run::{build, expect, within};
use crate::engine::{Engine, Limits};

/// A command that says one thing on each stream and fails with
/// [`EXIT_CODE`].
pub(super) const STREAMS: &str = "printf out; printf err >&2; exit 3";
/// What it says on each stream.
const STDOUT: &[u8] = b"out";
const STDERR: &[u8] = b"err";
/// The status it ends with.
const EXIT_CODE: i32 = 3;
/// Where the tree is imported, and exported from: a fresh workspace's root.
const ROOT: &str = ".";

/// One tree entry: its name, what it is, its mode, and its link target or
/// content.
type Planted = (&'static str, EntryType, u32, &'static [u8]);

/// A tree with modes kept apart, a relative link and an empty directory.
const TREE: &[Planted] = &[
    (
        "README.md",
        EntryType::Regular,
        0o644,
        b"# morning-rooster\n",
    ),
    ("bin", EntryType::Directory, 0o755, b""),
    (
        "bin/crow",
        EntryType::Regular,
        0o755,
        b"#!/bin/sh\necho crow\n",
    ),
    ("bin/readme", EntryType::Symlink, 0o777, b"../README.md"),
    ("empty", EntryType::Directory, 0o750, b""),
];

/// One entry as a listing compares it: path, kind, mode, link target and
/// content. A link's own mode means nothing and is left out.
type Listed = (String, u8, u32, String, Vec<u8>);

/// The engine grants the ask whole, enforcing what its capabilities say, or
/// refuses as they state it would; and the sandbox it builds says it
/// holds that grant, under that engine.
pub(super) async fn grant_is_stated(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let capabilities = engine.capabilities();
    let asked = Limits::DEFAULT;
    let granted = match engine.grant(asked) {
        Ok(granted) => granted,
        Err(refused) => {
            let stated = capabilities
                .grant(asked)
                .err()
                .map(|stated| stated.detail());
            return expect(
                stated.as_deref() == Some(refused.detail().as_str()),
                format!("a refusal is the one its capabilities state ({stated:?}): {refused}"),
            );
        }
    };
    expect(
        *granted.limits() == asked
            && granted.facts() == capabilities.resources
            && granted.engine() == capabilities.engine,
        format!("the grant is the ask, as {capabilities:?} states: {granted:?}"),
    )?;
    let sandbox = build(engine, asked).await?;
    let held = (*sandbox.granted(), sandbox.environment().engine);
    sandbox.destroy().await?;
    expect(
        held == (granted, capabilities.engine),
        format!("the sandbox holds its grant, under its engine: {held:?}"),
    )
}

/// A command's standard output and error arrive apart, and its exit code is
/// the one it exited with.
pub(super) async fn streams_and_exit_code(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let (ending, stdout, stderr) = within(build(engine, Limits::DEFAULT).await?, async |sandbox| {
        let spawn = Spawn::program("/bin/sh").arg("-c").arg(STREAMS);
        let process = sandbox.agent().spawn(&spawn).await?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let ending = process
            .ended(|stream, data| match stream {
                Stream::Stdout => stdout.extend_from_slice(&data),
                Stream::Stderr | Stream::Terminal => stderr.extend_from_slice(&data),
            })
            .await;
        Ok((ending, stdout, stderr))
    })
    .await?;
    expect(
        ending == Some(Ending::Exited(EXIT_CODE)),
        format!("it exits {EXIT_CODE}, got {ending:?}"),
    )?;
    expect(
        stdout == STDOUT && stderr == STDERR,
        format!(
            "each stream says its own, got {:?} and {:?}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        ),
    )
}

/// A tree imported into a fresh workspace exports as the same tree: every
/// path, kind, mode, link target and byte.
pub(super) async fn import_then_export(
    engine: &dyn Engine,
    _context: &Context<'_>,
) -> Result<(), Failed> {
    let tree = packed()?;
    let exported = within(build(engine, Limits::DEFAULT).await?, async |sandbox| {
        sandbox.agent().import(ROOT, &mut tree.as_slice()).await?;
        let mut exported = Vec::new();
        sandbox.agent().export(&[ROOT], &mut exported).await?;
        Ok(exported)
    })
    .await?;
    let (sent, back) = (listed(&tree)?, listed(&exported)?);
    expect(
        sent == back,
        format!("the tree comes back whole: sent {sent:?}, got {back:?}"),
    )
}

/// [`TREE`] as a tar stream.
fn packed() -> Result<Vec<u8>, Failed> {
    let mut builder = Builder::new(Vec::new());
    for &(name, kind, mode, data) in TREE {
        let mut header = Header::new_gnu();
        header.set_entry_type(kind);
        header.set_mode(mode);
        if kind == EntryType::Symlink {
            header.set_size(0);
            builder.append_link(&mut header, name, std::str::from_utf8(data)?)?;
        } else {
            header.set_size(u64::try_from(data.len())?);
            builder.append_data(&mut header, name, data)?;
        }
    }
    Ok(builder.into_inner()?)
}

/// `stream`'s entries, sorted, as [`Listed`].
fn listed(stream: &[u8]) -> Result<Vec<Listed>, Failed> {
    let mut listing = Vec::new();
    for entry in Archive::new(stream).entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        let path = entry
            .path()?
            .to_string_lossy()
            .trim_end_matches('/')
            .to_owned();
        let mode = if kind == EntryType::Symlink {
            0
        } else {
            entry.header().mode()? & 0o777
        };
        let target = entry
            .link_name()?
            .map_or_else(String::new, |target| target.to_string_lossy().into_owned());
        let mut content = Vec::new();
        entry.read_to_end(&mut content)?;
        listing.push((path, kind.as_byte(), mode, target, content));
    }
    listing.sort();
    Ok(listing)
}
