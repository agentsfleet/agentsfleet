//! Connected-server identity and topology proof for a bench target.

use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use super::{Datastores, command};
use crate::error::Result;
use crate::profile::Target;

/// The connected Postgres identity, with no configured URL or credential.
const POSTGRES_IDENTITY_QUERY: &str = "SELECT COALESCE(inet_server_addr()::text, 'local'), \
     COALESCE(inet_server_port(), 0), current_database(), version()";
const REDIS: &str = "redis";
const CLUSTER_DISABLED_DISPLAY: &str = "ResponseError: This instance has cluster support disabled";
const CONNECTED_REPLICAS: &str = "connected_slaves:";

/// Raw server and topology evidence collected from the connected datastores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatastoreProbe {
    /// Connected Postgres address, port, database and version.
    pub postgres_raw: String,
    /// `INFO server`, unmodified.
    pub redis_server_raw: String,
    /// `INFO replication`, unmodified.
    pub redis_replication_raw: String,
    /// `CLUSTER NODES`, or the exact refusal on a standalone server.
    pub redis_topology_raw: String,
    /// Every address advertised by Redis replication or cluster topology.
    pub discovered_hosts: Vec<String>,
}

impl Datastores {
    /// Refuse unsafe endpoints, connect, then verify server-reported topology.
    ///
    /// # Errors
    ///
    /// A datastore connection/probe failure, or [`crate::Error::UnsafeTarget`]
    /// when a local target advertises a remote node.
    pub async fn open_checked(
        target: &Target,
        database_url: &str,
        redis_url: &str,
        ca_cert: Option<String>,
    ) -> Result<Self> {
        let opened = Self::open(database_url, redis_url, ca_cert).await?;
        let _ = opened.probe(target).await?;
        Ok(opened)
    }

    /// Collect and validate connected-server identity and advertised topology.
    ///
    /// # Errors
    ///
    /// A datastore that will not answer, an unreadable Postgres identity, or a
    /// local target that reports any non-loopback host.
    pub async fn probe(&self, target: &Target) -> Result<DatastoreProbe> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(POSTGRES_IDENTITY_QUERY)
            .fetch_one(&mut *connection)
            .await?;
        let postgres_host: String = row.try_get(0)?;
        let postgres_port: i32 = row.try_get(1)?;
        let database: String = row.try_get(2)?;
        let postgres_version: String = row.try_get(3)?;
        let redis_server_raw = redis_info(self, command::SERVER).await?;
        let redis_replication_raw = redis_info(self, command::REPLICATION).await?;
        let mut cluster = redis::cmd(command::CLUSTER);
        cluster.arg(command::NODES);
        let (redis_topology_raw, cluster_hosts) = match self
            .queue
            .command::<String>(command::CLUSTER, command::NODES, &cluster)
            .await
        {
            Ok(raw) => {
                let hosts = cluster_hosts(&raw);
                (raw, hosts)
            }
            Err(failure) => match standalone_refusal(&failure) {
                Some(refusal) => (format!("refused: {refusal}"), Vec::new()),
                None => return Err(failure.into()),
            },
        };
        // `inet_server_addr()` is the server's own container-side interface,
        // not a client redirect or reconnect address. The configured Postgres
        // endpoint was already checked before this connection opened.
        let mut discovered_hosts = Vec::new();
        if let Some(master) = info_field(&redis_replication_raw, "master_host:") {
            discovered_hosts.push(master.to_owned());
        }
        discovered_hosts.extend(replication_hosts(&redis_replication_raw)?);
        discovered_hosts.extend(cluster_hosts);
        discovered_hosts.sort();
        discovered_hosts.dedup();
        for host in &discovered_hosts {
            target.check_discovered_host("advertised datastore node", host)?;
        }

        Ok(DatastoreProbe {
            postgres_raw: format!(
                "address={postgres_host}\nport={postgres_port}\ndatabase={database}\nversion={postgres_version}\n"
            ),
            redis_server_raw,
            redis_replication_raw,
            redis_topology_raw,
            discovered_hosts,
        })
    }
}

fn standalone_refusal(failure: &afd_redis::Error) -> Option<String> {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(failure);
    while let Some(error) = current {
        let rendered = error.to_string();
        if is_cluster_disabled(&rendered) {
            return Some(rendered);
        }
        current = error.source();
    }
    None
}

fn is_cluster_disabled(rendered: &str) -> bool {
    rendered == CLUSTER_DISABLED_DISPLAY
}

fn replication_hosts(raw: &str) -> Result<Vec<String>> {
    let expected = info_field(raw, CONNECTED_REPLICAS)
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(crate::Error::CounterUnreadable {
            datastore: REDIS,
            field: CONNECTED_REPLICAS,
        })?;
    let hosts: Vec<String> = raw
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(label, _fields)| replica_label(label))
        .filter_map(|(_label, fields)| {
            fields
                .split(',')
                .find_map(|field| field.strip_prefix("ip="))
                .map(str::to_owned)
        })
        .collect();
    if hosts.len() != expected {
        return Err(crate::Error::CounterUnreadable {
            datastore: REDIS,
            field: "replica addresses",
        });
    }
    Ok(hosts)
}

fn replica_label(label: &str) -> bool {
    ["slave", "replica"].iter().any(|prefix| {
        label.strip_prefix(prefix).is_some_and(|index| {
            !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

/// One raw `INFO` section.
async fn redis_info(stores: &Datastores, section: &'static str) -> Result<String> {
    let mut request = redis::cmd(command::INFO);
    request.arg(section);
    Ok(stores
        .queue
        .command(command::INFO, section, &request)
        .await?)
}

/// One `INFO` field, without changing its bytes beyond line endings.
fn info_field<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    raw.lines().find_map(|line| line.strip_prefix(prefix))
}

/// Hostnames and addresses advertised by `CLUSTER NODES`.
fn cluster_hosts(raw: &str) -> Vec<String> {
    raw.lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter_map(|address| address.split('@').next())
        .filter_map(|address| {
            if address.starts_with('[') {
                return address.find(']').map(|end| address[1..end].to_owned());
            }
            address
                .rsplit_once(':')
                .map(|(host, _port)| host.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests;
