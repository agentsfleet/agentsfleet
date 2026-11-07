//! What a lease's network needs from the engine: the resolver files its
//! sandbox reads, and, under an allowlist, the egress scope joining it to the
//! host once it runs.

use std::fs;
use std::os::fd::AsFd as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use crate::bubblewrap::NetworkLayout;
use crate::egress::{self, Scope};
use crate::error::Result;
use crate::network::{Allowlist, Network, RESOLV_CONF};

/// Where an allowlisted lease's rendered resolver files are written, in its
/// directory, before bubblewrap binds them over the image's own.
const HOSTS_FILE: &str = "hosts";
const RESOLV_CONF_FILE: &str = "resolv.conf";
/// The rendered files: readable by the sandbox, written by the runner alone.
const RESOLVER_FILE_MODE: u32 = 0o644;

/// Joins the running sandbox whose cgroup lists its processes at `procs` to the
/// host, admitting `allowlist`, off the async runtime: netlink blocks. The
/// blocking thread runs inside the caller's span, so a scope's refusal is
/// logged under the lease it refused.
pub(super) async fn join(procs: PathBuf, allowlist: Allowlist) -> Result<Scope> {
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || {
        span.in_scope(|| {
            let netns = egress::namespace_of(&procs)?;
            Scope::build(&egress::Host, netns.as_fd(), &allowlist)
        })
    })
    .await?
}

/// The resolver files a sandbox's network names, rendered where its policy
/// needs them.
#[derive(Debug)]
pub(super) enum Names {
    /// The host's own, or the image's: nothing to render.
    Kept(NetworkLayout<'static>),
    /// An allowlist's, rendered into the lease's directory.
    Rendered {
        hosts: PathBuf,
        resolv_conf: PathBuf,
    },
}

impl Names {
    /// Writes an allowlisted lease's `/etc/hosts` and resolver-less
    /// `/etc/resolv.conf` into `dir`; any other lease's names need nothing.
    pub(super) fn render(dir: &Path, network: Network<'_>) -> Result<Self> {
        let Network::Allowed(allowlist) = network else {
            let kept = match network {
                Network::Host => NetworkLayout::Host,
                Network::Isolated | Network::Allowed(_) => NetworkLayout::Isolated,
            };
            return Ok(Self::Kept(kept));
        };
        let (hosts, resolv_conf) = (dir.join(HOSTS_FILE), dir.join(RESOLV_CONF_FILE));
        for (path, text) in [
            (&hosts, allowlist.hosts_file()),
            (&resolv_conf, RESOLV_CONF.to_owned()),
        ] {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(RESOLVER_FILE_MODE)
                .open(path)
                .and_then(|mut file| std::io::Write::write_all(&mut file, text.as_bytes()))?;
        }
        Ok(Self::Rendered { hosts, resolv_conf })
    }

    /// Renders `allowlist`'s names over the `/etc/hosts` the sandbox in `dir`
    /// already reads. The file is truncated and written in place: the
    /// sandbox's bind holds the file, so a file written beside it and renamed
    /// over it would never be seen.
    pub(super) fn rewrite(dir: &Path, allowlist: &Allowlist) -> Result<()> {
        fs::write(dir.join(HOSTS_FILE), allowlist.hosts_file())?;
        Ok(())
    }

    /// What bubblewrap is told of them.
    pub(super) fn layout(&self) -> NetworkLayout<'_> {
        match self {
            Self::Kept(layout) => *layout,
            Self::Rendered { hosts, resolv_conf } => NetworkLayout::Allowed { hosts, resolv_conf },
        }
    }
}
