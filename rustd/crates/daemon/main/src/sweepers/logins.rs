//! Expired device-login sessions and connect nonces leave Postgres in batches.
//!
//! Both used to expire as datastore keys. As rows they wait for this pass;
//! every read already refuses an expired row, so a late pass costs space only.

use crate::supervisor::Supervisor;
use afd_db::Db;
use afd_tenant::session::SessionStore;
use std::time::Duration;

const INTERVAL: Duration = Duration::from_mins(1);
/// Rows one statement deletes, so a backlog never holds one long lock.
const BATCH: i64 = 1_000;

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db) {
    let database = database.clone();
    let sessions = SessionStore::new(database.clone());
    supervisor.spawn(super::LOGINS, async move |stop| {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { () = stop.cancelled() => return, _ = interval.tick() => {} }
            sweep(&sessions, &database).await;
        }
    });
}

/// One pass over both tables, logged when it removed anything or failed.
async fn sweep(sessions: &SessionStore, database: &Db) {
    match sessions.sweep_expired(BATCH).await {
        Ok(0) => {}
        Ok(swept) => tracing::info!(event = "login_sessions_swept", swept),
        Err(error) => {
            let error_code = error.code().as_str();
            tracing::warn!(event = "login_sessions_sweep_deferred", error_code);
        }
    }
    match afd_connector::state::nonce::sweep_expired(database, BATCH).await {
        Ok(0) => {}
        Ok(swept) => tracing::info!(event = "connect_nonces_swept", swept),
        Err(error) => {
            let error_code = error.code().as_str();
            tracing::warn!(event = "connect_nonces_sweep_deferred", error_code);
        }
    }
}
