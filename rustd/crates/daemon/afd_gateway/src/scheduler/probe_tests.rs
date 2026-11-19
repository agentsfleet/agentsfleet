//! One waiting probe skips idle tenants without dropping recoverable work.

use super::*;

type RecordedProbes = Arc<parking_lot::Mutex<Vec<Vec<Uuid7>>>>;
type SchedulerJob = tokio::task::JoinHandle<crate::Result<()>>;

// Value: protects=an unclaimable wake costs no discovery scan per connection;
// fails_when=the pass runs before or without the pending probe; why_new=the
// probe gate; seam=the injected probe; no_claim=recovery generations skip it.
#[tokio::test(start_paused = true)]
async fn a_wake_with_nothing_pending_asks_no_connection_to_claim() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut connection = ConnectionFixture::new(1)?;
    let _registration = connection.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let probes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let job = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        let probes = Arc::clone(&probes);
        async move {
            notifications(&registry, &wake, 8, stop, |_| {
                probes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async { Ok(Probe::Nothing) }
            })
            .await
        }
    });
    tokio::task::yield_now().await;
    wake.ring(connection.handle.runner.clone());
    let asked = tokio::select! {
        request = connection.requests.recv() => request.is_some(),
        () = tokio::time::sleep(Duration::from_secs(1)) => false,
    };
    assert!(!asked, "an empty probe skips the pass");
    assert_eq!(
        probes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "one probe per wake"
    );
    stop.cancel();
    job.await??;
    Ok(())
}

// Value: protects=a probe the datastore cannot answer never drops a wake;
// fails_when=a probe error skips the pass; why_new=the probe gate fails open;
// seam=the injected probe.
#[tokio::test(start_paused = true)]
async fn a_failed_probe_still_runs_the_pass() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut connection = ConnectionFixture::new(1)?;
    let _registration = connection.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let job = tokio::spawn({
        let registry = Arc::clone(&registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move {
            notifications(&registry, &wake, 8, stop, |_| async {
                Err(crate::Error::from(sqlx::Error::RowNotFound))
            })
            .await
        }
    });
    tokio::task::yield_now().await;
    wake.ring(connection.handle.runner.clone());
    connection
        .request()
        .await?
        .reply
        .send(OfferOutcome::Empty)
        .map_err(|unsent| format!("the pass lost its answer {unsent:?}"))?;
    stop.cancel();
    job.await??;
    Ok(())
}

// Value: protects=a wake costs a tenant runner with nothing to take no claim;
// fails_when=the pass offers a runner the probe named idle, or the probe is
// not asked about every connected runner; why_new=the idle skip; seam=the
// injected probe.
#[tokio::test(start_paused = true)]
async fn a_pass_skips_the_runner_the_probe_names_idle() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut busy = ConnectionFixture::new(1)?;
    let mut idle = ConnectionFixture::new(2)?;
    let _busy = busy.register(&registry)?;
    let _idle = idle.register(&registry)?;
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let (job, asked) = recording_scheduler(&registry, &wake, &stop, idle.handle.runner.clone());
    tokio::task::yield_now().await;
    wake.ring(busy.handle.runner.clone());
    busy.request()
        .await?
        .reply
        .send(OfferOutcome::Empty)
        .map_err(|unsent| format!("pass lost its empty answer {unsent:?}"))?;
    let reached_idle = tokio::select! {
        request = idle.requests.recv() => request.is_some(),
        () = tokio::time::sleep(Duration::from_secs(1)) => false,
    };
    assert!(
        !reached_idle,
        "a runner the probe named idle is never asked"
    );
    let probed: Vec<HashSet<Uuid7>> = asked
        .lock()
        .iter()
        .map(|runners| runners.iter().cloned().collect())
        .collect();
    assert_eq!(
        probed,
        [HashSet::from([
            busy.handle.runner.clone(),
            idle.handle.runner.clone()
        ])],
        "one probe per wake names every connected runner"
    );
    stop.cancel();
    job.await??;
    Ok(())
}

// Value: protects=the idle skip saves the scans it exists to save and strands
// no work; fails_when=a wake over fifty tenant runners asks more than the one
// with work, or a recovery pass skips anyone; why_new=the idle skip;
// seam=the injected probe and the recovery generation.
#[tokio::test(start_paused = true)]
async fn a_wake_asks_the_one_runner_with_work_and_a_recovery_pass_asks_all_fifty() -> TestResult {
    let registry = Arc::new(Registry::default());
    let mut connections = (1..=50)
        .map(ConnectionFixture::new)
        .collect::<TestResult<Vec<_>>>()?;
    let _registrations = connections
        .iter()
        .map(|connection| connection.register(&registry))
        .collect::<crate::Result<Vec<_>>>()?;
    let working = connections.get(6).ok_or("fifty connections")?;
    let working = working.handle.runner.clone();
    let idle: HashSet<Uuid7> = connections
        .iter()
        .map(|connection| connection.handle.runner.clone())
        .filter(|runner| *runner != working)
        .collect();
    let probes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let wake = Wake::default();
    let stop = CancellationToken::new();
    let job = idle_scheduler(&registry, &wake, &stop, idle, Arc::clone(&probes));
    tokio::task::yield_now().await;

    wake.ring(working.clone());
    connections
        .get_mut(6)
        .ok_or("fifty connections")?
        .request()
        .await?
        .reply
        .send(OfferOutcome::Empty)
        .map_err(|unsent| format!("pass lost its empty answer {unsent:?}"))?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let asked = connections
        .iter_mut()
        .map(|connection| connection.requests.try_recv().is_ok())
        .filter(|asked| *asked)
        .count();
    assert_eq!(asked, 0, "the 49 idle runners are asked nothing");
    assert_eq!(probes.load(std::sync::atomic::Ordering::SeqCst), 1);

    recovery_asks_every_connection(&wake, &mut connections).await?;
    assert_eq!(
        probes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "and does not probe"
    );
    stop.cancel();
    job.await??;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn candidates_leave_out_the_idle_runners() -> TestResult {
    let registry = Arc::new(Registry::default());
    let kept = ConnectionFixture::new(1)?;
    let idle = ConnectionFixture::new(2)?;
    let _kept = kept.register(&registry)?;
    let _idle = idle.register(&registry)?;
    let connections = registry.candidates(
        &HashSet::new(),
        &HashSet::from([idle.handle.runner.clone()]),
    );
    let order: Vec<_> = connections
        .iter()
        .map(|connection| &connection.runner)
        .collect();
    assert_eq!(order, [&kept.handle.runner]);
    Ok(())
}

fn recording_scheduler(
    registry: &Arc<Registry>,
    wake: &Wake,
    stop: &CancellationToken,
    named: Uuid7,
) -> (SchedulerJob, RecordedProbes) {
    let asked = Arc::new(parking_lot::Mutex::new(Vec::<Vec<Uuid7>>::new()));
    let job = tokio::spawn({
        let registry = Arc::clone(registry);
        let wake = wake.clone();
        let stop = stop.clone();
        let asked = Arc::clone(&asked);
        async move {
            notifications(&registry, &wake, 8, stop, |runners| {
                asked.lock().push(runners);
                let named = named.clone();
                async move { Ok(Probe::Waiting(HashSet::from([named]))) }
            })
            .await
        }
    });
    (job, asked)
}

fn idle_scheduler(
    registry: &Arc<Registry>,
    wake: &Wake,
    stop: &CancellationToken,
    idle: HashSet<Uuid7>,
    probes: Arc<std::sync::atomic::AtomicUsize>,
) -> SchedulerJob {
    tokio::spawn({
        let registry = Arc::clone(registry);
        let wake = wake.clone();
        let stop = stop.clone();
        async move {
            notifications(&registry, &wake, 8, stop, |_| {
                probes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let idle = idle.clone();
                async move { Ok(Probe::Waiting(idle)) }
            })
            .await
        }
    })
}

async fn recovery_asks_every_connection(
    wake: &Wake,
    connections: &mut [ConnectionFixture],
) -> TestResult {
    wake.recheck();
    let answered = tokio::time::timeout(
        Duration::from_secs(5),
        futures_util::future::try_join_all(connections.iter_mut().map(|connection| async move {
            let answer = connection.request().await?;
            assert_eq!(answer.mode, crate::connection::OfferMode::Recovery);
            answer
                .reply
                .send(OfferOutcome::Empty)
                .map_err(|unsent| format!("recovery pass lost its answer {unsent:?}"))?;
            TestResult::Ok(())
        })),
    )
    .await??;
    assert_eq!(answered.len(), 50, "a recovery pass asks every connection");
    Ok(())
}
