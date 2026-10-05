//! A working copy made from a mirror, without touching the network: the
//! mirror's objects copied in, its branches recorded as `origin`'s, the base
//! branch checked out, and every file handed to the sandbox's user.

use std::fs;
use std::os::unix::fs::lchown;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use afr_tools::sandbox::{GIT_IDENTITY_EMAIL, GIT_IDENTITY_NAME};
use gix::bstr::ByteSlice as _;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use gix::refs::{FullName, Target};

use super::fetch::{GitResult, ORIGIN};

/// Where a repository keeps its objects, under its git directory.
const OBJECTS: &str = "objects";
/// Where `origin`'s branches are recorded, in the mirror and the working copy
/// alike. A gix bare clone fetches every branch here and writes `refs/heads/`
/// once, at clone time, so the mirror's own branches go stale.
const ORIGIN_BRANCHES: &str = "refs/remotes/origin/";
/// Left out of the working copy: the clone writes it as a plain id and no
/// later fetch moves it, so it would name a stale commit.
const ORIGIN_HEAD: &str = "refs/remotes/origin/HEAD";
/// The working copy's own branches.
const LOCAL_BRANCHES: &str = "refs/heads/";
/// The reference naming what is checked out.
const HEAD: &str = "HEAD";
/// What every reference written here logs as its reason.
const REFLOG_MESSAGE: &str = "checked out from the runner's mirror";
/// The remote section a working copy's origin is configured under.
const REMOTE: &str = "remote";
/// The branch section a base branch's upstream is configured under.
const BRANCH: &str = "branch";
/// The keys of those sections the working copy sets.
const URL: &str = "url";
const FETCH: &str = "fetch";
const MERGE: &str = "merge";
/// The configuration file in a git directory.
const CONFIG: &str = "config";
/// Where `origin`'s branches are fetched to.
const ORIGIN_REFSPEC: &str = "+refs/heads/*:refs/remotes/origin/*";
/// The configuration keys the reference logs' committer is read from.
const COMMITTER_NAME: &str = "committer.name";
const COMMITTER_EMAIL: &str = "committer.email";

/// Makes `destination` a working copy of `mirror` at `base`, whose origin is
/// `url`, every file owned by `owner`. A read binding names no base, so an
/// empty `base` checks out the remote's default branch (`branch_of`).
pub(super) fn check_out(
    mirror: &Path,
    destination: &Path,
    url: &str,
    base: &str,
    owner: (u32, u32),
    stop: &AtomicBool,
) -> GitResult<()> {
    fs::create_dir(destination)?;
    let repository = gix::ThreadSafeRepository::init_opts(
        destination,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )?
    .to_thread_local();
    copy_tree(&mirror.join(OBJECTS), &repository.git_dir().join(OBJECTS))?;
    let source = gix::open_opts(mirror, gix::open::Options::isolated())?;
    let base = branch_of(&source, base)?;
    let repository = gix::open_opts(
        destination,
        gix::open::Options::isolated().config_overrides([
            format!("{COMMITTER_NAME}={GIT_IDENTITY_NAME}"),
            format!("{COMMITTER_EMAIL}={GIT_IDENTITY_EMAIL}"),
        ]),
    )?;
    record_branches(&source, &repository, &base)?;
    configure_origin(&repository, url, &base)?;
    write_worktree(&repository, &base, stop)?;
    hand_over(destination, owner)
}

/// `base`, or for a binding that names none, the branch the mirror's `HEAD`
/// names: the remote's default branch when the mirror was first cloned, which
/// is the one `git clone` would have checked out.
fn branch_of(source: &gix::Repository, base: &str) -> GitResult<String> {
    if !base.is_empty() {
        return Ok(base.to_owned());
    }
    let head = source
        .head_name()?
        .ok_or("the repository's HEAD names no branch")?;
    Ok(head.shorten().to_str()?.to_owned())
}

/// Records every branch the mirror fetched as `origin`'s, then `base` as the
/// one local branch with `HEAD` pointing at it: one transaction, so a failure
/// leaves no half.
fn record_branches(
    source: &gix::Repository,
    repository: &gix::Repository,
    base: &str,
) -> GitResult<()> {
    let base_ref = format!("{ORIGIN_BRANCHES}{base}");
    let mut edits = Vec::new();
    let mut base_tip = None;
    for reference in source.references()?.prefixed(ORIGIN_BRANCHES)? {
        let reference = reference?;
        if reference.name().as_bstr() == ORIGIN_HEAD {
            continue;
        }
        let target = reference.target().into_owned();
        if reference.name().as_bstr() == base_ref.as_str() {
            base_tip = target.try_id().map(ToOwned::to_owned);
        }
        edits.push(update(reference.name().to_owned(), target));
    }
    let tip = base_tip.ok_or_else(|| format!("the repository has no branch {base}"))?;
    let local: FullName = format!("{LOCAL_BRANCHES}{base}").try_into()?;
    edits.push(update(local.clone(), Target::Object(tip)));
    edits.push(update(HEAD.try_into()?, Target::Symbolic(local)));
    repository.edit_references(edits)?;
    Ok(())
}

/// An edit setting `name` to `target`, whatever it held, logged as the
/// runner's checkout.
fn update(name: FullName, target: Target) -> RefEdit {
    RefEdit {
        change: Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: REFLOG_MESSAGE.into(),
            },
            expected: PreviousValue::Any,
            new: target,
        },
        name,
        deref: false,
    }
}

/// Names `url` as `origin`, with no credential in it, and makes it `base`'s
/// upstream, so `git status` reads like any fresh clone's.
fn configure_origin(repository: &gix::Repository, url: &str, base: &str) -> GitResult<()> {
    let path = repository.git_dir().join(CONFIG);
    let mut config =
        gix::config::File::from_path_no_includes(path.clone(), gix::config::Source::Local)?;
    config.set_raw_value_by(REMOTE, Some(ORIGIN.into()), URL, url)?;
    config.set_raw_value_by(REMOTE, Some(ORIGIN.into()), FETCH, ORIGIN_REFSPEC)?;
    config.set_raw_value_by(BRANCH, Some(base.into()), REMOTE, ORIGIN)?;
    let merge = format!("{LOCAL_BRANCHES}{base}");
    config.set_raw_value_by(BRANCH, Some(base.into()), MERGE, merge.as_str())?;
    let mut file = fs::File::create(&path)?;
    config.write_to(&mut file)?;
    Ok(())
}

/// Writes `base`'s tree into the working directory and its index beside it.
fn write_worktree(repository: &gix::Repository, base: &str, stop: &AtomicBool) -> GitResult<()> {
    let workdir = repository
        .workdir()
        .ok_or("the working copy has no working directory")?;
    let tree = repository
        .find_reference(format!("{LOCAL_BRANCHES}{base}").as_str())?
        .peel_to_id()?
        .object()?
        .peel_to_tree()?
        .id;
    let mut index = repository.index_from_tree(&tree)?;
    let mut options =
        repository.checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping)?;
    options.destination_is_initially_empty = true;
    gix::worktree::state::checkout(
        &mut index,
        workdir,
        repository.objects.clone().into_arc()?,
        &gix::progress::Discard,
        &gix::progress::Discard,
        stop,
        options,
    )?;
    index.write(gix::index::write::Options::default())?;
    Ok(())
}

/// Copies every file under `from` to the same place under `to`.
fn copy_tree(from: &Path, to: &Path) -> GitResult<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Hands `path` and everything under it to `owner`, never following a link.
fn hand_over(path: &Path, owner: (u32, u32)) -> GitResult<()> {
    lchown(path, Some(owner.0), Some(owner.1))?;
    if path.symlink_metadata()?.is_dir() {
        for entry in fs::read_dir(path)? {
            hand_over(&entry?.path(), owner)?;
        }
    }
    Ok(())
}
