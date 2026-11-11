//! What the relay cases share: servers, relays and one transfer per case.

use super::path_limit::{Path, PathLimit, Traffic};
use crate::support::{Fixture, Host, Result};
use af_stream::{
    CHUNK_BYTES, Connection, Endpoint, IncomingReader, Outgoing, Transport,
    test_util::STREAM_WINDOW, tls::Trust,
};
use af_wire::protocol::{STREAM_KIND_HEADER, StreamKind};
use http::Request;
use std::{error::Error, fmt, num::NonZeroUsize, time::Duration};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _, DuplexStream},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

/// The path MTU `ip -6 route get` reports from the dev runner host toward the daemon.
pub(super) const MEASURED_PATH_MTU: usize = 1370;
/// The smallest MTU every IPv6 link must carry.
pub(super) const IPV6_MIN_MTU: usize = 1280;
/// The IPv6 header plus the UDP header: what separates a path's MTU from the payload it carries.
const IPV6_UDP_HEADERS: usize = 48;
/// What tokio-quiche's HTTP/3 server sends when nothing sets it.
const SERVER_DEFAULT_DATAGRAM: usize = 1350;
/// What quiche tells a peer it accepts when nothing sets it.
const CLIENT_DEFAULT_RECEIVE: usize = 65_527;
/// The slowest healthy case finishes in about three seconds; a stalled one never does.
pub(super) const CASE_DEADLINE: Duration = Duration::from_secs(10);
/// Loss costs retransmission rounds, so it gets room to finish.
pub(super) const LOSSY_DEADLINE: Duration = Duration::from_secs(30);
/// A transfer that must stall is shown stalling, not waited out.
pub(super) const STALL_DEADLINE: Duration = Duration::from_secs(3);
/// The lossy path loses every seventh datagram over a kilobyte, in each direction.
pub(super) const LOSS_EVERY: NonZeroUsize = NonZeroUsize::MIN.saturating_add(6);

/// What each endpoint sends or accepts when it holds no ceiling of its own, to model a build that
/// lacks one. Only the test hooks can set either.
#[derive(Clone, Copy)]
pub(super) struct Builds {
    daemon_sends: Option<usize>,
    runner_accepts: Option<usize>,
}

impl Builds {
    /// This repository's daemon and runner.
    pub(super) const CURRENT: Self = Self {
        daemon_sends: None,
        runner_accepts: None,
    };
    pub(super) const DAEMON_WITHOUT_CEILING: Self = Self {
        daemon_sends: Some(SERVER_DEFAULT_DATAGRAM),
        runner_accepts: None,
    };
    pub(super) const RUNNER_WITHOUT_CEILING: Self = Self {
        daemon_sends: None,
        runner_accepts: Some(CLIENT_DEFAULT_RECEIVE),
    };
    pub(super) const BOTH_WITHOUT_CEILING: Self = Self {
        daemon_sends: Some(SERVER_DEFAULT_DATAGRAM),
        runner_accepts: Some(CLIENT_DEFAULT_RECEIVE),
    };
}

pub(super) fn path_payload_limit(path_mtu: usize) -> usize {
    path_mtu - IPV6_UDP_HEADERS
}

pub(super) fn stream_window() -> Result<usize> {
    Ok(usize::try_from(STREAM_WINDOW)?)
}

/// One packet, one chunk, one byte past the stream window, 250 KB, 1 MiB and 2 MiB.
pub(super) fn all_sizes() -> Result<Vec<usize>> {
    Ok(vec![
        1_024,
        CHUNK_BYTES,
        stream_window()? + 1,
        250_000,
        1 << 20,
        2 << 20,
    ])
}

/// The sizes that cross several windows, for the cases that cost more per run.
pub(super) fn bulk_sizes() -> Result<Vec<usize>> {
    Ok(vec![stream_window()? + 1, 250_000, 1 << 20])
}

/// Bytes that no repeating pattern reproduces, so a lost or reordered range shows in the compare.
pub(super) fn payload(size: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_u32;
    (0..size)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state.to_be_bytes()[0]
        })
        .collect()
}

/// Where a transfer stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Connect,
    Open,
    Transfer,
}

/// A transfer that stopped, with the stage it stopped in and the error behind it.
#[derive(Debug)]
pub(super) struct Failure {
    pub(super) stage: Stage,
    source: Box<dyn Error + Send + Sync>,
}

impl Failure {
    fn new(stage: Stage, source: impl Into<Box<dyn Error + Send + Sync>>) -> Self {
        Self {
            stage,
            source: source.into(),
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.stage, self.source)
    }
}

impl Error for Failure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.source()
    }
}

type Attempt<T> = std::result::Result<T, Failure>;

pub(super) struct Lane {
    writer: DuplexStream,
    reader: IncomingReader,
}

impl Lane {
    /// The server sends back every byte it receives, so both directions carry `payload`.
    pub(super) async fn echo(&mut self, payload: &[u8], deadline: Instant) -> Attempt<Vec<u8>> {
        let mut answer = vec![0; payload.len()];
        let transfer = async {
            tokio::try_join!(
                self.writer.write_all(payload),
                self.reader.read_exact(&mut answer)
            )
        };
        let finished = tokio::time::timeout_at(deadline, transfer).await;
        match finished {
            Ok(Ok(_)) => Ok(answer),
            Ok(Err(error)) => Err(Failure::new(Stage::Transfer, error)),
            Err(elapsed) => Err(Failure::new(Stage::Transfer, elapsed)),
        }
    }
}

pub(super) async fn connect(
    trust: &Trust,
    endpoint: &Endpoint,
    deadline: Instant,
) -> Attempt<Connection> {
    let stop = CancellationToken::new();
    Connection::connect(Transport::H3, endpoint, trust, &stop, deadline)
        .await
        .map_err(|error| Failure::new(Stage::Connect, error))
}

async fn open_lane(
    connection: &mut Connection,
    endpoint: &Endpoint,
    kind: StreamKind,
    deadline: Instant,
) -> Attempt<Lane> {
    let (writer, body) = Outgoing::pipe();
    let request = Request::builder()
        .method("POST")
        .uri(endpoint.uri().clone())
        .header(STREAM_KIND_HEADER, kind.as_str())
        .body(body)
        .map_err(|error| Failure::new(Stage::Open, error))?;
    let response = connection
        .open(request, deadline)
        .await
        .map_err(|error| Failure::new(Stage::Open, error))?;
    Ok(Lane {
        writer,
        reader: response.into_body().reader(),
    })
}

/// One lane of each kind on one connection, as production opens them.
pub(super) async fn open_all_lanes(
    connection: &mut Connection,
    endpoint: &Endpoint,
    deadline: Instant,
) -> Attempt<[Lane; 4]> {
    let control = open_lane(connection, endpoint, StreamKind::Control, deadline).await?;
    let work = open_lane(connection, endpoint, StreamKind::Work, deadline).await?;
    let reports = open_lane(connection, endpoint, StreamKind::Reports, deadline).await?;
    let activity = open_lane(connection, endpoint, StreamKind::Activity, deadline).await?;
    Ok([control, work, reports, activity])
}

#[derive(Debug)]
pub(super) enum Verdict {
    Arrived,
    Altered,
    Stalled(Failure),
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrived => write!(f, "arrived"),
            Self::Altered => write!(f, "came back altered"),
            Self::Stalled(failure) => write!(f, "stalled at {failure}"),
        }
    }
}

pub(super) struct Outcome {
    pub(super) size: usize,
    pub(super) verdict: Verdict,
    pub(super) traffic: Traffic,
}

struct Case {
    size: usize,
    fixture: Fixture,
    trust: Trust,
    host: Host,
    path: PathLimit,
}

/// Bind every listener first, then start the relays: a relay's ephemeral UDP port then cannot be
/// the number a later listener's TCP socket receives.
async fn prepare(sizes: &[usize], path: Path, builds: Builds) -> Result<Vec<Case>> {
    let mut bound = Vec::with_capacity(sizes.len());
    for &size in sizes {
        let fixture = Fixture::new(vec!["localhost".to_owned()], false)?;
        let host = match builds.daemon_sends {
            Some(bytes) => fixture.serve_mode(false, Some(bytes)).await?,
            None => fixture.serve().await?,
        };
        let trust = match builds.runner_accepts {
            Some(bytes) => fixture.trust.clone().with_receive_limit(bytes),
            None => fixture.trust.clone(),
        };
        bound.push((size, fixture, trust, host));
    }
    let mut cases = Vec::with_capacity(bound.len());
    for (size, fixture, trust, host) in bound {
        let relay = PathLimit::start(&host, path).await?;
        cases.push(Case {
            size,
            fixture,
            trust,
            host,
            path: relay,
        });
    }
    Ok(cases)
}

async fn transfer(
    trust: &Trust,
    path: &PathLimit,
    sent: &[u8],
    deadline: Instant,
    shrink_to: Option<usize>,
) -> Attempt<Vec<u8>> {
    let mut connection = connect(trust, &path.endpoint, deadline).await?;
    let mut work = open_lane(&mut connection, &path.endpoint, StreamKind::Work, deadline).await?;
    if let Some(bytes) = shrink_to {
        path.set_limit(bytes);
    }
    work.echo(sent, deadline).await
}

async fn run(case: Case, patience: Duration, shrink_to: Option<usize>) -> Result<Outcome> {
    let Case {
        size,
        fixture: _fixture,
        trust,
        host: _host,
        path,
    } = case;
    let sent = payload(size);
    let deadline = Instant::now() + patience;
    let verdict = match transfer(&trust, &path, &sent, deadline, shrink_to).await {
        Ok(answer) if answer == sent => Verdict::Arrived,
        Ok(_) => Verdict::Altered,
        Err(failure) => Verdict::Stalled(failure),
    };
    Ok(Outcome {
        size,
        verdict,
        traffic: path.traffic(),
    })
}

/// Every size at once, each on its own server, relay and connection. `shrink_to` lowers the
/// path's limit after the handshake; `builds` picks which daemon and runner meet.
pub(super) async fn matrix(
    sizes: &[usize],
    path: Path,
    patience: Duration,
    shrink_to: Option<usize>,
    builds: Builds,
) -> Result<Vec<Outcome>> {
    let cases = prepare(sizes, path, builds).await?;
    let tasks: Vec<_> = cases
        .into_iter()
        .map(|case| tokio::spawn(run(case, patience, shrink_to)))
        .collect();
    let mut outcomes = Vec::with_capacity(tasks.len());
    for task in tasks {
        outcomes.push(task.await??);
    }
    Ok(outcomes)
}
