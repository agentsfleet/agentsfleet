//! The scripted daemon answers current session frames through the real correlator.

use std::{borrow::Cow, sync::Arc};

use af_stream::{Budget, MemoryLimits, OwnedFrame};
use af_wire::{
    paths::RUNNER_STREAM,
    protocol::{Down, Up},
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

use crate::{
    Result,
    client::{Call, RunnerApi, Verb},
    downlink::{pending::Requests, running::Running},
    error,
    uplink::{
        SessionSender, StreamHandle,
        queues::{Outbound, Queues, Receivers},
    },
};

pub(super) fn start(api: &Arc<dyn RunnerApi>, stop: CancellationToken) -> StreamHandle {
    let (queues, receivers) = Queues::new();
    let (budget, _) = Budget::duplex(MemoryLimits::default());
    let requests = Requests::new();
    let sender = Arc::new(SessionSender {
        queues,
        budget: budget.clone(),
        requests: Arc::clone(&requests),
        stop: stop.clone(),
    });
    let (renewal_owner, renewal_session) = watch::channel(Some(Arc::clone(&sender)));
    let (current, session) = watch::channel(Some(sender));
    let (assignment, assignments) = watch::channel(None);
    let handle = StreamHandle::new(session, Arc::new(Running::default()), assignments);
    let Receivers {
        priority,
        control,
        work,
        reports,
        activity,
    } = receivers;
    for queue in [priority, control, work, reports, activity] {
        tokio::spawn(serve(
            queue,
            Arc::clone(api),
            Arc::clone(&requests),
            budget.clone(),
            stop.clone(),
        ));
    }
    let mut renewal_handle = StreamHandle::new(
        renewal_session,
        Arc::clone(&handle.running),
        handle.assignments.clone(),
    );
    renewal_handle.renewing = Arc::clone(&handle.renewing);
    tokio::spawn(async move {
        let _assignment = assignment;
        // The batcher uses its own receiver so it cannot keep the caller's lifetime watch alive.
        let _renewal_owner = renewal_owner;
        tokio::select! {
            () = current.closed() => {},
            _finished = renewal_handle.renewing.run(&renewal_handle, stop.clone()) => {},
        }
        stop.cancel();
        requests.close();
    });
    handle
}

async fn serve(
    mut queue: mpsc::Receiver<Outbound>,
    api: Arc<dyn RunnerApi>,
    requests: Arc<Requests>,
    budget: Budget,
    stop: CancellationToken,
) {
    let mut workers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            done = workers.join_next(), if !workers.is_empty() => {
                if !matches!(done, Some(Ok(Ok(())))) { stop.cancel(); break; }
            }
            outbound = queue.recv(), if workers.len() < af_wire::protocol::PENDING_MAX => {
                let Some(outbound) = outbound else { break; };
                workers.spawn(respond(Arc::clone(&api), Arc::clone(&requests), budget.clone(), stop.clone(), outbound));
            }
        }
    }
    requests.close();
    workers.shutdown().await;
}

async fn respond(
    api: Arc<dyn RunnerApi>,
    requests: Arc<Requests>,
    budget: Budget,
    stop: CancellationToken,
    outbound: Outbound,
) -> Result<()> {
    if let Some(message) = response(api.as_ref(), &outbound.frame).await? {
        let frame = af_stream::encode(
            &message,
            &budget,
            outbound.frame.decode::<Up<'_>>()?.stream_kind(),
            &stop,
            Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT,
        )
        .await?;
        let message: Down<'_> = frame.decode()?;
        let reply = requests.reply(&message)?;
        if let Some(reply) = reply {
            reply.send(frame);
        }
    }
    Ok(())
}

async fn response(api: &dyn RunnerApi, frame: &OwnedFrame) -> Result<Option<Value>> {
    let reply = match frame.decode::<Up<'_>>()? {
        Up::Heartbeat {
            request_id,
            request,
        } => json!({"type":"heartbeat", "request_id":request_id,
            "answer": answer(api, Verb::Heartbeat, &request).await?}),
        Up::Report {
            request_id,
            request,
        } => report_reply(api, request_id, &request).await?,
        Up::Folds {
            request_id,
            lease_id,
            request,
        } => folds_reply(api, request_id, &lease_id, &request).await?,
        Up::RenewBatch { request_id, leases } => renewals(api, request_id, leases).await?,
        Up::AuthoringModel {
            request_id,
            lease_id,
            request,
        } => json!({"type":"authoring_model_result", "request_id":request_id,
            "lease_id":lease_id, "answer":answer(api, Verb::AuthoringModel, &request).await?}),
        Up::AuthoringActivity {
            request_id,
            lease_id,
            request,
        } => json!({"type":"authoring_activity_ack", "request_id":request_id,
            "lease_id":lease_id, "answer":answer(api, Verb::AuthoringActivity, &request).await?}),
        Up::AuthoringRenew {
            request_id,
            lease_id,
            request,
        } => json!({"type":"authoring_renewed", "request_id":request_id,
            "lease_id":lease_id, "answer":answer(api, Verb::AuthoringRenew, &request).await?}),
        Up::AuthoringReport {
            request_id,
            lease_id,
            request,
        } => json!({"type":"authoring_report_ack", "request_id":request_id,
            "lease_id":lease_id, "answer":answer(api, Verb::AuthoringReport, &request).await?}),
        Up::Ping { request_id } => json!({"type":"pong", "request_id":request_id}),
        Up::Activity { request, .. } => return activity_reply(api, &request).await,
        Up::Ready { .. } => return Ok(None),
        Up::Hello { .. } | Up::Confirm { .. } => {
            panic!("opening is tested by the native connection fixtures")
        }
    };
    Ok(Some(reply))
}

async fn answer(api: &dyn RunnerApi, verb: Verb, request: &impl Serialize) -> Result<Value> {
    let body = Some(bytes::Bytes::from(
        serde_json::to_vec(request).map_err(error::encode)?,
    ));
    match api
        .send(Call {
            verb,
            path: Cow::Borrowed(RUNNER_STREAM),
            body,
        })
        .await
    {
        Ok(body) => Ok(json!({"status":"ok", "response":
            af_core::json::object_from_slice::<Value>(&body).map_err(error::malformed(verb))?})),
        Err(failure) => Ok(
            json!({"status":"refused", "error_code":failure.code().as_str(),
            "detail":"scripted daemon refusal", "retryable":failure.is_retryable()}),
        ),
    }
}

async fn activity_reply(
    api: &dyn RunnerApi,
    request: &af_wire::activity::ActivityRequest<'_>,
) -> Result<Option<Value>> {
    answer(api, Verb::Activity, request).await?;
    Ok(None)
}

async fn renewals(
    api: &dyn RunnerApi,
    request_id: std::num::NonZeroU64,
    leases: Vec<af_wire::protocol::Renewal<'_>>,
) -> Result<Value> {
    let mut answers = Vec::new();
    for lease in leases {
        answers.push(
            json!({"lease_id":lease.lease_id, "answer":answer(api, Verb::Renew, &lease).await?}),
        );
    }
    Ok(json!({"type":"renewed", "request_id":request_id, "leases":answers}))
}

async fn report_reply(
    api: &dyn RunnerApi,
    request_id: std::num::NonZeroU64,
    request: &af_wire::report::ReportRequest<'_>,
) -> Result<Value> {
    Ok(json!({"type":"report_ack", "request_id":request_id,
        "lease_id":request.lease_id, "answer":answer(api, Verb::Report, request).await?}))
}

async fn folds_reply(
    api: &dyn RunnerApi,
    request_id: std::num::NonZeroU64,
    lease_id: &str,
    request: &af_wire::fold::FoldRequest,
) -> Result<Value> {
    Ok(json!({"type":"folded", "request_id":request_id,
        "lease_id":lease_id, "answer":answer(api, Verb::Folds, request).await?}))
}
