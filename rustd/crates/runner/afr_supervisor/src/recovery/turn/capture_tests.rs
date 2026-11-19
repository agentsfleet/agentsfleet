//! A file-backed ordinary capture registers its chunks, uploads the missing
//! ones, writes its pages and commits; every refusal leaves no committed head.

use super::*;
use af_wire::{
    run::{TurnCheckpointPlan, TurnResume},
    task::CheckpointTransferPurpose,
};
use afr_agent::TurnSnapshot;
use std::sync::{Arc, Mutex};

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The private root every capture here carries.
const HISTORY: &[u8] = b"private root";

/// How the native owner is told to misbehave for one capture.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    /// Another writer holds the first chunk.
    BusyChunk,
    /// The allocation names one page fewer than the capture asked for.
    ShortPlan,
    /// The page acknowledgement carries other bytes than the page written.
    ChangedPage,
    /// The allocation is answered with another command's response.
    WrongBegin,
    /// A renewal answers with a pin the capture was never given.
    ForeignPin,
    /// A page write is answered with another command's response.
    WrongPageWrite,
    /// The commit is answered with another command's response.
    WrongCommit,
    /// The commit acknowledges another generation than the one allocated.
    ChangedCommit,
}

/// What the native owner saw of the captures it served.
struct Remote {
    objects: Vec<CheckpointObject>,
    fault: Option<Fault>,
    plan: Option<TurnCheckpointPlan>,
    pages: Vec<CheckpointPage>,
    head: Option<TurnResume>,
    uploaded: bool,
    upload_requests: usize,
    abandoned: usize,
    /// How long the allocation's pin lasts, and how long the whole capture may.
    custody: (Duration, Duration),
    /// The pin the owner currently holds for the allocation.
    pin: Option<CheckpointPin>,
    renewals: usize,
}

impl Remote {
    /// An owner holding `objects`, each uploaded to its own path under `url`.
    fn new(objects: Vec<CheckpointObject>, url: &str, fault: Option<Fault>) -> Checked<Self> {
        let mut placed = Vec::with_capacity(objects.len());
        for mut object in objects {
            let mut target = url::Url::parse(url)?;
            target.set_path(&format!("/{}", object.object_id));
            object.transfer = Some(CheckpointTransfer {
                url: target.to_string(),
                headers: BTreeMap::from([("if-none-match".to_owned(), "*".to_owned())]),
                expires_at: clock::after(clock::now(), Duration::from_secs(30)),
            });
            placed.push(object);
        }
        Ok(Self {
            objects: placed,
            fault,
            plan: None,
            pages: Vec::new(),
            head: None,
            uploaded: false,
            upload_requests: 0,
            abandoned: 0,
            custody: (Duration::from_mins(15), Duration::from_mins(15)),
            pin: None,
            renewals: 0,
        })
    }

    /// Extends the held pin by a minute, as the owner does for a live capture.
    fn renew(&mut self, presented: u64) -> Result<Response> {
        let held = self
            .pin
            .as_mut()
            .ok_or_else(|| error::config("renewal without an allocation"))?;
        if presented != held.resource_version {
            return Err(error::config("renewal presented a stale pin version"));
        }
        self.renewals += 1;
        held.resource_version += 1;
        held.expires_at =
            clock::after(clock::now(), Duration::from_mins(1)).min(held.operation_deadline);
        let mut answered = held.clone();
        if self.fault == Some(Fault::ForeignPin) {
            answered.id = ID.to_owned();
        }
        Ok(Response::PinRenewed(answered))
    }

    fn respond(&mut self, request: TurnCheckpointCommand) -> Result<Response> {
        match request.action {
            Action::Begin(begin) => Ok(self.begin(begin)),
            Action::Objects { objects, .. } => objects
                .iter()
                .enumerate()
                .map(|(index, wanted)| {
                    let mut object = self.holding(|held| held.plaintext.sha256 == wanted.sha256)?;
                    object.transfer = None;
                    object.status = if self.fault == Some(Fault::BusyChunk) && index == 0 {
                        CheckpointObjectStatus::Busy
                    } else if self.uploaded {
                        CheckpointObjectStatus::Reused
                    } else {
                        CheckpointObjectStatus::Missing
                    };
                    Ok(object)
                })
                .collect::<Result<_>>()
                .map(Response::Objects),
            Action::Uploads { objects, .. } => {
                self.upload_requests += 1;
                objects
                    .iter()
                    .map(|wanted| {
                        let mut object = self.holding(|held| held.object_id == wanted.object_id)?;
                        object.status = CheckpointObjectStatus::Missing;
                        Ok(object)
                    })
                    .collect::<Result<_>>()
                    .map(Response::Uploads)
            }
            Action::Authorize {
                purpose: CheckpointTransferPurpose::Upload,
                object_ids,
                ..
            } => {
                self.uploaded = true;
                object_ids
                    .iter()
                    .map(|wanted| {
                        let mut object = self.holding(|held| held.object_id == wanted.0)?;
                        object.status = CheckpointObjectStatus::Reused;
                        object.transfer = None;
                        Ok(object)
                    })
                    .collect::<Result<_>>()
                    .map(Response::Authorized)
            }
            Action::PageWrite { .. } if self.fault == Some(Fault::WrongPageWrite) => {
                Ok(Response::PinReleased)
            }
            Action::PageWrite { contents, .. } => {
                self.pages.push(contents.clone());
                Ok(Response::PageWritten(
                    if self.fault == Some(Fault::ChangedPage) {
                        page(Vec::new())
                    } else {
                        contents
                    },
                ))
            }
            Action::Commit { .. } if self.fault == Some(Fault::WrongCommit) => {
                Ok(Response::PinReleased)
            }
            Action::Commit { manifest_root, .. } => {
                let plan = self
                    .plan
                    .as_ref()
                    .ok_or_else(|| error::config("commit without an allocation"))?;
                let saved = TurnResume {
                    checkpoint_id: plan.checkpoint_id.clone(),
                    generation: plan.generation,
                    format_version: plan.format_version,
                    scope: plan.scope.clone(),
                    manifest_root,
                    history_digest: plan.history_digest.clone(),
                };
                if self.fault == Some(Fault::ChangedCommit) {
                    return Ok(Response::Committed(Box::new(TurnResume {
                        generation: saved.generation + 1,
                        ..saved
                    })));
                }
                self.head = Some(saved.clone());
                Ok(Response::Committed(Box::new(saved)))
            }
            Action::PinRenew {
                resource_version, ..
            } => self.renew(resource_version),
            Action::Abandon { .. } => {
                self.abandoned += 1;
                self.plan = None;
                Ok(Response::Abandoned)
            }
            _ => Err(error::config("unexpected ordinary capture request")),
        }
    }

    fn holding(&self, wanted: impl Fn(&CheckpointObject) -> bool) -> Result<CheckpointObject> {
        self.objects
            .iter()
            .find(|held| wanted(held))
            .cloned()
            .ok_or_else(|| error::config("the capture named a chunk this fixture does not hold"))
    }

    fn begin(&mut self, begin: af_wire::run::TurnCheckpointBegin) -> Response {
        if self.fault == Some(Fault::WrongBegin) {
            return Response::Abandoned;
        }
        let generation = self.head.as_ref().map_or(1, |head| head.generation + 1);
        let pages = if self.fault == Some(Fault::ShortPlan) {
            begin.page_count.saturating_sub(1)
        } else {
            begin.page_count
        };
        let plan = TurnCheckpointPlan {
            checkpoint_id: TurnId(format!("019f0000-0000-7000-8000-{:012}", generation + 20)),
            generation,
            format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
            scope: scope(),
            expected_head: begin.expected_head,
            page_ids: (0..pages)
                .map(|index| TurnId(format!("019f0000-0000-7000-8000-{:012}", index + 100)))
                .collect(),
            history_digest: begin.history_digest,
            replayed: false,
            pin: CheckpointPin {
                id: PAGE.to_owned(),
                resource_version: 1,
                expires_at: clock::after(clock::now(), self.custody.0),
                operation_deadline: clock::after(clock::now(), self.custody.1),
            },
        };
        self.pin = Some(plan.pin.clone());
        self.plan = Some(plan.clone());
        Response::Begun(Box::new(plan))
    }
}

fn capture_plane(remote: Arc<Mutex<Remote>>) -> (crate::client::ControlPlane, test_support::Calls) {
    test_support::plane(move |call| {
        let result = (|| -> Result<Response> {
            let body = call
                .body
                .as_ref()
                .ok_or_else(|| error::config("missing command"))?;
            let request: TurnCheckpointCommand =
                serde_json::from_slice(body).map_err(error::encode)?;
            remote
                .lock()
                .map_err(|_poisoned| error::config("the fixture owner panicked"))?
                .respond(request)
        })();
        match result {
            Ok(response) => test_support::json(&response),
            Err(failure) => Answer::Fail(failure),
        }
    })
}

fn snapshot(scope: TurnScope) -> TurnSnapshot {
    TurnSnapshot {
        scope,
        previous: None,
        history: Zeroizing::new(HISTORY.to_vec()),
        folds: Vec::new(),
    }
}

/// A workspace of four one-chunk files, one per fixture object.
fn workspace() -> Checked<tempfile::TempDir> {
    let root = tempfile::tempdir()?;
    for index in 0..4 {
        std::fs::write(
            root.path().join(format!("file-{index}")),
            format!("part-{index}\n"),
        )?;
    }
    Ok(root)
}

/// Every file a capture's pages name, with its size.
fn files(pages: &[CheckpointPage]) -> Vec<(String, u64)> {
    let mut named: Vec<_> = pages
        .iter()
        .flat_map(|page| &page.entries)
        .filter_map(|entry| match entry {
            CheckpointEntry::File(file) => Some((file.path.clone(), file.size)),
            CheckpointEntry::Directory { .. }
            | CheckpointEntry::Symlink { .. }
            | CheckpointEntry::Remove { .. } => None,
        })
        .collect();
    named.sort();
    named
}

#[tokio::test]
async fn a_file_backed_capture_uploads_each_missing_chunk_and_commits_its_pages() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let mut server = Server::simultaneous(payloads.clone()).await?;
    let root = workspace()?;
    let remote = Arc::new(Mutex::new(Remote::new(objects, &server.url, None)?));
    let (plane, _calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();

    let saved = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await?;

    for _ in 0..4 {
        let seen = server.seen.try_recv()?;
        let path = seen.path.split('?').next().ok_or("missing request path")?;
        assert_eq!(seen.method, axum::http::Method::PUT);
        assert_eq!(
            seen.bytes.as_ref(),
            payloads.get(path).ok_or("unknown PUT")?
        );
    }
    let (head, abandoned, uploads, named) = {
        let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
        (
            owner.head.clone(),
            owner.abandoned,
            owner.upload_requests,
            files(&owner.pages),
        )
    };
    assert_eq!(head.as_ref(), Some(&saved));
    assert_eq!(abandoned, 0);
    assert_eq!(uploads, 1, "four small chunks share one batch");
    assert_eq!(
        named,
        (0..4)
            .map(|index| (format!("file-{index}"), 7))
            .collect::<Vec<_>>()
    );
    assert_eq!(saved.generation, 1);
    assert_eq!(saved.scope, scope());
    assert_eq!(saved.history_digest, af_core::etag::sha256_hex(HISTORY));

    // The next capture finds every chunk stored: it uploads nothing and
    // advances the generation from the acknowledged head.
    let again = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await?;
    assert_eq!(again.generation, 2);
    assert!(
        server.seen.try_recv().is_err(),
        "a stored chunk is not sent again"
    );
    assert_eq!(
        remote
            .lock()
            .map_err(|_poisoned| "the owner panicked")?
            .upload_requests,
        1
    );

    // A capture under another scope than the acknowledged head is refused
    // before it asks the owner for anything.
    let mut foreign = scope();
    foreign.fleet_id = TurnId(PAGE.to_owned());
    let refused = native
        .capture(snapshot(foreign), Some(root.path().to_owned()), &stop)
        .await;
    assert!(refused.is_err(), "a changed scope must not capture");
    assert_eq!(
        remote
            .lock()
            .map_err(|_poisoned| "the owner panicked")?
            .head
            .as_ref(),
        Some(&again)
    );
    server.finish().await
}

#[tokio::test]
async fn a_capture_the_owner_cannot_honour_is_abandoned_without_a_head() -> Checked {
    for fault in [Fault::BusyChunk, Fault::ShortPlan] {
        let Encrypted { objects, .. } = encrypted()?;
        let root = workspace()?;
        let remote = Arc::new(Mutex::new(Remote::new(
            objects,
            "https://objects.invalid",
            Some(fault),
        )?));
        let (plane, _calls) = capture_plane(Arc::clone(&remote));
        let run = Uuid7::parse(ID)?;
        let transfers = Transfers::new()?;
        let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
        let stop = CancellationToken::new();

        let refused = native
            .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
            .await;

        assert!(refused.is_err(), "the capture must fail");
        let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
        assert!(owner.head.is_none(), "a refused capture commits nothing");
        assert_eq!(owner.abandoned, 1, "the allocation is given back once");
        assert_eq!(owner.upload_requests, 0, "nothing is uploaded");
    }
    Ok(())
}

#[tokio::test]
async fn a_page_acknowledged_with_other_bytes_ends_the_capture() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let server = Server::simultaneous(payloads).await?;
    let root = workspace()?;
    let remote = Arc::new(Mutex::new(Remote::new(
        objects,
        &server.url,
        Some(Fault::ChangedPage),
    )?));
    let (plane, _calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();

    let refused = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await;

    assert!(refused.is_err(), "changed page bytes must not commit");
    let (head, abandoned, pages) = {
        let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
        (owner.head.clone(), owner.abandoned, owner.pages.len())
    };
    assert!(head.is_none());
    assert_eq!(abandoned, 1);
    assert_eq!(pages, 1, "the capture stops at the first page");
    server.finish().await
}

#[tokio::test]
async fn a_capture_refuses_a_wrong_reply_or_a_root_its_scope_does_not_name() -> Checked {
    let Encrypted { objects, .. } = encrypted()?;
    let root = workspace()?;
    let remote = Arc::new(Mutex::new(Remote::new(
        objects,
        "https://objects.invalid",
        Some(Fault::WrongBegin),
    )?));
    let (plane, mut calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();

    // An allocation answered with another command's response.
    let Err(_refused) = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await
    else {
        return Err("a wrong allocation reply was accepted".into());
    };
    assert_eq!(
        test_support::drain(&mut calls).len(),
        1,
        "only the allocation was sent"
    );

    // A files scope with no root, and an API-only scope handed one.
    let mut api_only = scope();
    api_only.workspace = TurnWorkspace::ApiOnly;
    for (scope, root) in [(scope(), None), (api_only, Some(root.path().to_owned()))] {
        let Err(_refused) = native.capture(snapshot(scope), root, &stop).await else {
            return Err("a root its scope does not name was captured".into());
        };
    }
    assert!(
        test_support::drain(&mut calls).is_empty(),
        "a mismatched root never reaches the owner"
    );
    assert!(
        remote
            .lock()
            .map_err(|_poisoned| "the owner panicked")?
            .head
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn a_capture_whose_pin_is_about_to_lapse_renews_custody_as_it_works() -> Checked {
    let Encrypted { objects, payloads } = encrypted()?;
    let server = Server::simultaneous(payloads).await?;
    let root = workspace()?;
    let mut owner = Remote::new(objects, &server.url, None)?;
    owner.custody.0 = Duration::from_secs(30);
    let remote = Arc::new(Mutex::new(owner));
    let (plane, _calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = server.transfers();
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();

    let saved = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await?;

    let (head, renewals, version) = {
        let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
        (
            owner.head.clone(),
            owner.renewals,
            owner.pin.as_ref().map(|pin| pin.resource_version),
        )
    };
    assert_eq!(head.as_ref(), Some(&saved));
    assert!(
        renewals > 1,
        "each step renews a pin inside its last 90 seconds"
    );
    assert_eq!(
        version,
        u64::try_from(renewals).ok().map(|renewed| renewed + 1),
        "every renewal presented the version the owner held"
    );
    server.finish().await
}

#[tokio::test]
async fn a_capture_stops_when_custody_changes_hands_or_has_ended() -> Checked {
    for (fault, pin_lasts, capture_lasts) in [
        (
            Some(Fault::ForeignPin),
            Duration::from_secs(30),
            Duration::from_mins(15),
        ),
        (None, Duration::ZERO, Duration::ZERO),
    ] {
        let Encrypted { objects, .. } = encrypted()?;
        let root = workspace()?;
        let mut owner = Remote::new(objects, "https://objects.invalid", fault)?;
        owner.custody = (pin_lasts, capture_lasts);
        let remote = Arc::new(Mutex::new(owner));
        let (plane, _calls) = capture_plane(Arc::clone(&remote));
        let run = Uuid7::parse(ID)?;
        let transfers = Transfers::new()?;
        let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
        let stop = CancellationToken::new();

        let refused = native
            .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
            .await;

        assert!(
            refused.is_err(),
            "custody the capture does not hold must end it"
        );
        let (head, abandoned, uploads) = {
            let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
            (owner.head.clone(), owner.abandoned, owner.upload_requests)
        };
        assert!(head.is_none(), "nothing is committed without custody");
        assert_eq!(abandoned, 1, "the allocation is given back once");
        assert_eq!(uploads, 0, "no chunk is sent without custody");
    }
    Ok(())
}

#[tokio::test]
async fn a_root_that_is_not_utf8_is_abandoned_before_its_commit() -> Checked {
    let root = tempfile::tempdir()?;
    let remote = Arc::new(Mutex::new(Remote::new(
        Vec::new(),
        "https://objects.invalid",
        None,
    )?));
    let (plane, _calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();
    let mut binary = snapshot(scope());
    binary.history = Zeroizing::new(vec![0xff, 0xfe]);

    let refused = native
        .capture(binary, Some(root.path().to_owned()), &stop)
        .await;

    assert_eq!(
        test_support::detail(&refused),
        Some("ordinary root is not UTF-8")
    );
    let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
    assert!(owner.head.is_none());
    assert_eq!(owner.pages.len(), 1, "the page was written before the root");
    assert_eq!(owner.abandoned, 1);
    Ok(())
}

/// A commit receipt of the right kind that names another generation is
/// refused: the capture keeps no head and leaves no uncertain commit behind.
#[tokio::test]
async fn a_commit_receipt_naming_another_generation_commits_no_head() -> Checked {
    // An empty workspace needs no chunk, so the capture goes straight to its
    // one page and then its commit.
    let root = tempfile::tempdir()?;
    let remote = Arc::new(Mutex::new(Remote::new(
        Vec::new(),
        "https://objects.invalid",
        Some(Fault::ChangedCommit),
    )?));
    let (plane, _calls) = capture_plane(Arc::clone(&remote));
    let run = Uuid7::parse(ID)?;
    let transfers = Transfers::new()?;
    let mut native = NativeTurn::new(&plane, &run, 1, &transfers);
    let stop = CancellationToken::new();

    let refused = native
        .capture(snapshot(scope()), Some(root.path().to_owned()), &stop)
        .await;

    assert_eq!(
        test_support::detail(&refused),
        Some("turn commit changed the captured generation or root")
    );
    let (head, pages) = {
        let owner = remote.lock().map_err(|_poisoned| "the owner panicked")?;
        (owner.head.clone(), owner.pages.len())
    };
    assert!(head.is_none(), "the owner recorded no head");
    assert_eq!(pages, 1, "the one page was written");
    assert!(native.publication.head.is_none(), "no head is kept");
    assert!(
        native.publication.pending.is_none(),
        "a refused receipt leaves no uncertain commit to reconcile"
    );
    Ok(())
}
