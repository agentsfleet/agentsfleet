use super::Renewals;
use af_core::id::Uuid7;
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;

mod recovery;

#[tokio::test(start_paused = true)]
async fn configured_host_renewal_interval_reaches_the_native_queue() -> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let (handle, mut peer) = RenewalPeer::new();
    let run = Uuid7::parse(crate::test_util::RUN_ID)?;
    let started = Instant::now();
    let _registration = registry.register(
        &run,
        Arc::new(afr_agent::Meter::default()),
        started + Duration::from_secs(28),
    )?;
    let settings = garde::Unvalidated::new(crate::config::RuntimeSettings {
        renewal_interval_ms: 1000,
        ..crate::config::RuntimeSettings::default()
    })
    .validate()?;
    let policy = crate::config::RuntimePolicy::new(&settings);
    let stop = tokio_util::sync::CancellationToken::new();
    let running = registry.run_with_interval(
        &handle,
        stop.clone(),
        Duration::from_millis(policy.settings().renewal_interval_ms),
    );
    let observed = async {
        let message = peer
            .outgoing
            .priority
            .recv()
            .await
            .ok_or(crate::error::ErrorKind::Disconnected)?;
        assert_eq!(started.elapsed(), Duration::from_secs(1));
        let af_wire::protocol::Up::RenewBatch { runs, .. } =
            message.frame.decode::<af_wire::protocol::Up<'_>>()?
        else {
            return Err(crate::error::ErrorKind::StreamProtocol.into());
        };
        assert_eq!(runs.len(), 1);
        stop.cancel();
        Ok::<(), crate::Error>(())
    };
    let (ran, observed) = tokio::join!(running, observed);
    ran?;
    observed
}

#[tokio::test(start_paused = true)]
async fn recovery_shares_confirmed_deadlines_and_retires_them_before_final_delivery()
-> crate::Result<()> {
    let registry = Arc::new(Renewals::default());
    let fleet = Uuid7::parse(crate::test_util::RUN_ID)?;
    let second = Uuid7::parse("019a3c00-0000-7000-8000-0000000000a2")?;
    let now = Instant::now();
    let fleet = registry.register(
        &fleet,
        Arc::new(afr_agent::Meter::default()),
        now + Duration::from_secs(3),
    )?;
    let second = registry
        .register(
            &second,
            Arc::new(afr_agent::Meter::default()),
            now + Duration::from_secs(2),
        )?
        .lifetime;
    assert_eq!(
        registry.recovery_deadline(now + Duration::from_secs(7)),
        now + Duration::from_secs(2)
    );
    second.extend(now + Duration::from_secs(6));
    assert_eq!(
        registry.recovery_deadline(now + Duration::from_secs(7)),
        now + Duration::from_secs(3)
    );
    drop(fleet);
    assert_eq!(
        registry.recovery_deadline(now + Duration::from_secs(7)),
        now + Duration::from_secs(6)
    );
    tokio::time::advance(Duration::from_secs(6)).await;
    drop(second);
    assert_eq!(
        registry.recovery_deadline(Instant::now() + Duration::from_secs(7)),
        now + Duration::from_secs(13)
    );
    Ok(())
}

/// A full, legal host must renew through the real encoder, priority queue and request registry.
#[tokio::test(start_paused = true)]
async fn full_host_usage_renews_in_bounded_independently_correlated_frames() -> crate::Result<()> {
    let host = FullHost::new()?;
    let (handle, mut peer) = RenewalPeer::new();
    let snapshot = host.registry.snapshot();
    let oversized: af_wire::protocol::Up<'_> = af_wire::protocol::Up::RenewBatch {
        request_id: std::num::NonZeroU64::MAX,
        runs: snapshot.into_iter().map(|(_, renewal)| renewal).collect(),
    };
    assert!(
        serde_json::to_vec(&oversized)
            .map_err(crate::error::encode)?
            .len()
            > af_wire::protocol::CONTROL_MESSAGE_BYTES_MAX,
        "this legal host population must exercise the frame partition"
    );
    drop(oversized);
    let (renewed, answered) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(host.registry.batch(&handle), peer.answer_snapshot())
    })
    .await?;
    renewed?;
    assert!(answered? > 1, "the full host needs several bounded frames");
    assert!(!peer.sender.stop.is_cancelled());
    for (index, registration) in host.registrations.iter().enumerate() {
        let expected = u32::try_from(index).map_err(crate::error::stream_protocol)?;
        let Some(super::Renewed::Confirmed(answer)) = registration.updates.borrow().clone() else {
            return Err(crate::error::ErrorKind::StreamProtocol.into());
        };
        assert_eq!(answer.recorded_through, Some(expected));
        assert_eq!(registration.lifetime.deadline(), host.deadline);
    }
    Ok(())
}

struct FullHost {
    registry: Arc<Renewals>,
    registrations: Vec<super::Registration>,
    deadline: Instant,
}

impl FullHost {
    fn new() -> crate::Result<Self> {
        let registry = Arc::new(Renewals::default());
        let deadline = Instant::now() + Duration::from_secs(28);
        let mut registrations = Vec::new();
        for index in 0..af_wire::protocol::RUNS_MAX {
            let id = Uuid7::parse(&format!("019f0000-0000-7000-8000-{index:012x}"))?;
            let meter = Arc::new(afr_agent::Meter::new(Arc::new(
                af_core::clock::FixedClock::at(jiff::Timestamp::UNIX_EPOCH),
            )));
            for _ in 0..=af_wire::usage::LINES_MAX {
                meter.add(guffbolt::Usage {
                    input: u64::from(u32::MAX),
                    cached_input: u64::from(u32::MAX),
                    cache_write: u64::from(u32::MAX),
                    cache_write_5m: u64::from(u32::MAX),
                    cache_write_1h: u64::from(u32::MAX),
                    output: u64::from(u32::MAX),
                });
            }
            registrations.push(registry.register(&id, meter, deadline)?);
        }
        Ok(Self {
            registry,
            registrations,
            deadline,
        })
    }
}

struct RenewalPeer {
    sender: Arc<super::super::SessionSender>,
    outgoing: super::super::queues::Receivers,
    current: tokio::sync::watch::Sender<Option<Arc<super::super::SessionSender>>>,
    _policies: tokio::sync::watch::Sender<Option<Arc<af_stream::OwnedFrame>>>,
}

impl RenewalPeer {
    fn new() -> (super::super::StreamHandle, Self) {
        let (queues, outgoing) = super::super::queues::Queues::new();
        let (_, output) = af_stream::Budget::duplex(af_stream::MemoryLimits::default());
        let sender = Arc::new(super::super::SessionSender {
            requests: crate::downlink::pending::Requests::new(),
            queues,
            budget: output,
            stop: tokio_util::sync::CancellationToken::new(),
        });
        let (current, session) = tokio::sync::watch::channel(Some(Arc::clone(&sender)));
        let (policies, watching) = tokio::sync::watch::channel(None);
        let handle = super::super::StreamHandle::new(
            session,
            Arc::new(crate::downlink::running::Running::default()),
            watching,
        );
        (
            handle,
            Self {
                sender,
                outgoing,
                current,
                _policies: policies,
            },
        )
    }

    /// Read all partitions before answering in reverse order, retaining only small replies.
    async fn answer_snapshot(&mut self) -> crate::Result<usize> {
        use af_wire::protocol::{CONTROL_MESSAGE_BYTES_MAX, RUNS_MAX, Up};
        let mut seen = std::collections::HashSet::new();
        let mut request_ids = std::collections::HashSet::new();
        let mut responses = Vec::new();
        let deadline = Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
        while seen.len() < RUNS_MAX {
            let message = self
                .outgoing
                .priority
                .recv()
                .await
                .ok_or(crate::error::ErrorKind::Disconnected)?;
            assert!(message.frame.as_bytes().len() <= CONTROL_MESSAGE_BYTES_MAX);
            assert_eq!(
                message.deadline, deadline,
                "partitions share one answer window"
            );
            let Up::RenewBatch { request_id, runs } = message.frame.decode::<Up<'_>>()? else {
                return Err(crate::error::ErrorKind::StreamProtocol.into());
            };
            assert!(
                request_ids.insert(request_id),
                "each batch owns its correlation id"
            );
            let mut answers = Vec::new();
            for renewal in runs.into_iter().rev() {
                let id = Uuid7::parse(&renewal.run_id)?;
                assert!(seen.insert(id));
                assert_eq!(renewal.request.requests.0.len(), af_wire::usage::LINES_MAX);
                let index = u32::from_str_radix(
                    renewal
                        .run_id
                        .rsplit('-')
                        .next()
                        .ok_or(crate::error::ErrorKind::StreamProtocol)?,
                    16,
                )
                .map_err(crate::error::stream_protocol)?;
                answers.push(serde_json::json!({"run_id":renewal.run_id,
                    "answer":{"status":"ok","response":{
                        "run_expires_at":"2030-01-01T00:00:00Z","recorded_through":index}}}));
            }
            responses.push(serde_json::json!({"type":"renewed","request_id":request_id,
                "runs":answers}));
        }
        let batches = responses.len();
        for response in responses.into_iter().rev() {
            self.deliver(&response).await?;
        }
        Ok(batches)
    }

    async fn deliver(&self, response: &serde_json::Value) -> crate::Result<()> {
        let frame = af_stream::encode(
            response,
            &af_stream::Budget::default(),
            af_wire::protocol::StreamKind::Control,
            &self.sender.stop,
            Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
        )
        .await?;
        let reply = frame.decode::<af_wire::protocol::Down<'_>>()?;
        self.sender
            .requests
            .reply(&reply)?
            .ok_or(crate::error::ErrorKind::StreamProtocol)?
            .send(frame);
        Ok(())
    }
}
