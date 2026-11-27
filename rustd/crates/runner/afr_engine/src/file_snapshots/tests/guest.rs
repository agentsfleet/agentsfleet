//! A guest agent whose workspace is a map: it pins by packing what changed
//! since the generation its last released pin committed, renews by emptying
//! it, and imports a tar stream back into it.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::sync::{Mutex, MutexGuard, PoisonError};

use af_guest_wire::Imported;
use af_guest_wire::protocol::PinWalked;
use afr_guest_client::{GuestAgent, Process, ProcessId, Spawn};
use bytes::Bytes;
use tar::{Archive, Builder, EntryType, Header};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// One entry of the fake's workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Node {
    Directory,
    Symlink(String),
    File {
        bytes: Vec<u8>,
        executable: bool,
    },
    /// A second name of the file named here.
    SecondName(String),
}

/// What the fake holds and what was asked of it.
#[derive(Debug, Default)]
pub(super) struct Held {
    pub(super) nodes: BTreeMap<String, Node>,
    pub(super) frozen: u32,
    pub(super) thawed: u32,
    pub(super) renewed: u32,
    /// The parent each walk was asked against.
    pub(super) walked: Vec<Option<String>>,
    /// The generation each release named.
    pub(super) released: Vec<Option<String>>,
    pins: u64,
    pending: BTreeMap<u64, BTreeMap<String, Node>>,
    committed: Option<(String, BTreeMap<String, Node>)>,
}

/// The fake guest agent, with the failures a test switches on.
#[derive(Debug, Default)]
pub(super) struct Guest {
    pub(super) held: Mutex<Held>,
    pub(super) freeze_fails: bool,
    pub(super) release_fails: bool,
}

impl Guest {
    pub(super) fn held(&self) -> MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts `node` at `path`.
    pub(super) fn put(&self, path: &str, node: Node) {
        self.held().nodes.insert(path.to_owned(), node);
    }
}

/// A failure the fake answers with.
fn refused() -> af_guest_wire::Error {
    std::io::Error::other("refused by the fake").into()
}

/// `nodes` packed as a walk packs them.
fn packed(nodes: &BTreeMap<String, Node>) -> std::io::Result<Vec<u8>> {
    let mut builder = Builder::new(Vec::new());
    for (path, node) in nodes {
        let mut header = Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        match node {
            Node::Directory => {
                header.set_entry_type(EntryType::Directory);
                builder.append_data(&mut header, path, std::io::empty())?;
            }
            Node::Symlink(target) => {
                header.set_entry_type(EntryType::Symlink);
                builder.append_link(&mut header, path, target)?;
            }
            Node::File { bytes, executable } => {
                header.set_mode(if *executable { 0o755 } else { 0o644 });
                header.set_size(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
                builder.append_data(&mut header, path, bytes.as_slice())?;
            }
            Node::SecondName(first) => {
                header.set_entry_type(EntryType::Link);
                builder.append_link(&mut header, path, first)?;
            }
        }
    }
    builder.into_inner()
}

#[async_trait::async_trait]
impl GuestAgent for Guest {
    async fn spawn(&self, _spawn: &Spawn) -> af_guest_wire::Result<Process> {
        Err(refused())
    }

    async fn write(&self, _process: ProcessId, _data: Bytes) -> af_guest_wire::Result<()> {
        Err(refused())
    }

    async fn kill(&self, _process: ProcessId) -> af_guest_wire::Result<()> {
        Err(refused())
    }

    async fn freeze_tenant(&self) -> af_guest_wire::Result<()> {
        if self.freeze_fails {
            return Err(refused());
        }
        self.held().frozen += 1;
        Ok(())
    }

    async fn thaw_tenant(&self) -> af_guest_wire::Result<()> {
        self.held().thawed += 1;
        Ok(())
    }

    async fn pin_walk(
        &self,
        parent: Option<&str>,
        into: &mut (dyn AsyncWrite + Send + Unpin),
    ) -> af_guest_wire::Result<PinWalked> {
        let (stream, walked) = {
            let held = &mut *self.held();
            held.walked.push(parent.map(str::to_owned));
            let base = held
                .committed
                .as_ref()
                .filter(|(generation, _)| Some(generation.as_str()) == parent)
                .map(|(_, nodes)| nodes.clone());
            let changed: BTreeMap<String, Node> = held
                .nodes
                .iter()
                .filter(|(path, node)| base.as_ref().and_then(|base| base.get(*path)) != Some(node))
                .map(|(path, node)| (path.clone(), node.clone()))
                .collect();
            let removed = base
                .iter()
                .flat_map(|base| base.keys())
                .filter(|path| !held.nodes.contains_key(*path))
                .cloned()
                .collect();
            held.pins += 1;
            held.pending.insert(held.pins, held.nodes.clone());
            let walked = PinWalked {
                pin: held.pins,
                transfer_id: 0,
                delta: base.is_some(),
                removed,
                entries: u64::try_from(held.nodes.len()).unwrap_or(u64::MAX),
                changed_bytes: 0,
            };
            (packed(&changed)?, walked)
        };
        into.write_all(&stream).await?;
        Ok(walked)
    }

    async fn pin_release(&self, pin: u64, generation: Option<&str>) -> af_guest_wire::Result<()> {
        let held = &mut *self.held();
        held.released.push(generation.map(str::to_owned));
        let pinned = held.pending.remove(&pin);
        if let (Some(generation), Some(pinned)) = (generation, pinned) {
            held.committed = Some((generation.to_owned(), pinned));
        }
        if self.release_fails {
            return Err(refused());
        }
        Ok(())
    }

    async fn renew(&self) -> af_guest_wire::Result<()> {
        let held = &mut *self.held();
        held.nodes.clear();
        held.renewed += 1;
        Ok(())
    }

    async fn import(
        &self,
        _at: &str,
        tree: &mut (dyn AsyncRead + Send + Unpin),
    ) -> af_guest_wire::Result<Imported> {
        let mut stream = Vec::new();
        tree.read_to_end(&mut stream).await?;
        let mut archive = Archive::new(stream.as_slice());
        let held = &mut *self.held();
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry
                .path()?
                .to_string_lossy()
                .trim_end_matches('/')
                .to_owned();
            let node = match entry.header().entry_type() {
                EntryType::Directory => Node::Directory,
                EntryType::Symlink => {
                    let target = entry.link_name()?.unwrap_or_default();
                    Node::Symlink(target.to_string_lossy().into_owned())
                }
                _file => {
                    let executable = entry.header().mode()? & 0o111 != 0;
                    let mut bytes = Vec::new();
                    entry.read_to_end(&mut bytes)?;
                    Node::File { bytes, executable }
                }
            };
            held.nodes.insert(path, node);
        }
        Ok(Imported::default())
    }
}
