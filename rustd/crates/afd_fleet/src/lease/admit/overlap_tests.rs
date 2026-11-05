//! The three money reads start together.
//!
//! A local listener stands in for Postgres: it accepts every connection and
//! never answers, so a ledger read that has started stays open until its
//! client gives up. The balance double holds its own read until two ledger
//! reads are open AT ONCE. Reads run one after another never get there: the
//! second could not start before the balance read ended, and a read that timed
//! out on its own has already closed its connection.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::io::Read as _;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_billing::biller::{
    Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use afd_core::env::MapEnv;
use afd_db::config::{DbRole, PoolConfig};
use afd_fleet_runtime::FleetConfig;
use tokio::sync::watch;

use super::{Admission, Request, money_gates};
use crate::lease::test_dead;

/// The ledger reads a pass starts beside the balance read: the catalogue
/// estimate and the fleet's spend.
const LEDGER_READS: usize = 2;

/// How long the held balance read waits for them. Generous: overlapping reads
/// connect in the same poll that starts the balance read.
const OVERLAP_WITHIN: Duration = Duration::from_secs(5);

/// A Postgres that accepts and never answers, counting the connections open
/// right now.
#[derive(Debug)]
struct Silent {
    port: u16,
    open: watch::Receiver<usize>,
    held: Arc<Mutex<Vec<TcpStream>>>,
    closed: Arc<AtomicBool>,
}

impl Silent {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let port = listener.local_addr().expect("the bound port").port();
        let (count, open) = watch::channel(0);
        let count = Arc::new(count);
        let (held, closed) = (
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(AtomicBool::new(false)),
        );
        let (handles, hung_up) = (Arc::clone(&held), Arc::clone(&closed));
        std::thread::spawn(move || {
            for mut socket in listener.incoming().map_while(Result::ok) {
                let handle = socket.try_clone().expect("a socket handle");
                if hung_up.load(Ordering::SeqCst) {
                    drop(handle.shutdown(Shutdown::Both));
                    continue;
                }
                handles
                    .lock()
                    .expect("the handle list is healthy")
                    .push(handle);
                count.send_modify(|open| *open += 1);
                let count = Arc::clone(&count);
                // Open until the client hangs up or `hang_up` shuts it.
                std::thread::spawn(move || {
                    let mut sink = [0_u8; 64];
                    while matches!(socket.read(&mut sink), Ok(read) if read > 0) {}
                    count.send_modify(|open| *open -= 1);
                });
            }
        });
        Self {
            port,
            open,
            held,
            closed,
        }
    }

    /// Shuts every open connection and every later one, so each read fails at
    /// once instead of waiting out its acquire budget.
    fn hang_up(&self) {
        self.closed.store(true, Ordering::SeqCst);
        for socket in self
            .held
            .lock()
            .expect("the handle list is healthy")
            .drain(..)
        {
            drop(socket.shutdown(Shutdown::Both));
        }
    }
}

/// The dead-ledger biller, except that its balance read is held until both
/// ledger reads are open together or [`OVERLAP_WITHIN`] passes.
#[derive(Debug)]
struct HeldBalance {
    silent: Silent,
    overlapped: AtomicBool,
    inner: afd_billing::ledger::LedgerBiller,
}

#[async_trait::async_trait]
impl Biller for HeldBalance {
    async fn reserve(&self, ask: &Reservation) -> afd_billing::Result<Hold> {
        self.inner.reserve(ask).await
    }

    async fn settle(&self, settlement: &Settlement) -> afd_billing::Result<Receipt> {
        self.inner.settle(settlement).await
    }

    async fn release(&self, work: &WorkKey, why: Release) -> afd_billing::Result<()> {
        self.inner.release(work, why).await
    }

    async fn balance(&self, payer: &Account) -> afd_billing::Result<Balance> {
        let mut open = self.silent.open.clone();
        let overlapped = matches!(
            tokio::time::timeout(OVERLAP_WITHIN, open.wait_for(|open| *open >= LEDGER_READS)).await,
            Ok(Ok(_))
        );
        self.overlapped.store(overlapped, Ordering::SeqCst);
        self.silent.hang_up();
        self.inner.balance(payer).await
    }
}

#[tokio::test]
async fn test_money_gate_reads_start_together() {
    let silent = Silent::start();
    let url = format!(
        "postgres://runner:secret@127.0.0.1:{}/agentsfleet",
        silent.port
    );
    let env = MapEnv::from_pairs([(DbRole::Api.url_knob(), url.as_str())]);
    let pool = PoolConfig::resolve(&env, DbRole::Api).expect("the listener's URL is well-formed");
    let accounts = afd_billing::Accounts::new(afd_db::Db::unreachable(&pool));
    let biller = HeldBalance {
        silent,
        overlapped: AtomicBool::new(false),
        inner: test_dead::biller(),
    };
    let config = FleetConfig::authored(
        r#"{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#,
    )
    .expect("the fixture document is authorable");
    let (workspace, fleet, tenant) = (test_dead::id(2), test_dead::id(1), test_dead::id(3));

    let decided = money_gates(
        &accounts,
        &biller,
        Ok(Some(tenant)),
        Request {
            workspace_id: &workspace,
            fleet_id: &fleet,
            event_id: "1767225600000-1",
            budget: config.budget(),
            posture: afd_billing::rates::Posture::Platform,
            provider: "anthropic",
            model: "claude-fixture",
        },
        test_dead::AT,
    )
    .await
    .expect("every fault here is absorbed into a decision");

    assert!(
        biller.overlapped.load(Ordering::SeqCst),
        "the estimate and spend reads were open together while the balance read was held"
    );
    assert!(
        matches!(decided, Admission::Admit(_)),
        "the failed reads fail open: {decided:?}"
    );
}
