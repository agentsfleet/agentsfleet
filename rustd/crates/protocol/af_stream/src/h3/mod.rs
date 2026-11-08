mod client;
mod driver;
mod headers;
mod server;
mod socket;
mod telemetry;
mod trust;

use bytes::Bytes;
use futures_util::SinkExt;
use tokio_quiche::http3::{
    driver::{OutboundFrame, OutboundFrameSender},
    settings::Http3Settings,
};

use crate::{Outgoing, Result};

pub(crate) use client::{Client, connect};
pub(crate) use server::serve;
pub use telemetry::NetworkStats;

fn settings() -> Http3Settings {
    let mut settings = Http3Settings::default();
    settings.max_requests_per_connection = Some(u64::from(crate::STREAMS));
    settings.max_header_list_size = Some(u64::from(crate::HEADER_BYTES));
    settings.qpack_max_table_capacity = Some(0);
    settings.qpack_blocked_streams = Some(0);
    settings.post_accept_timeout = Some(std::time::Duration::from_secs(5));
    settings.enable_extended_connect = false;
    settings.max_recv_body_buf_size = Some(crate::CHUNK_BYTES);
    settings
}

async fn upload(mut body: Outgoing, mut output: OutboundFrameSender) -> Result<()> {
    while let Some(mut bytes) = body.data().await? {
        while !bytes.is_empty() {
            let length = bytes.len().min(crate::CHUNK_BYTES);
            output
                .send(OutboundFrame::Body(bytes.split_to(length), false))
                .await?;
        }
    }
    output.send(OutboundFrame::Body(Bytes::new(), true)).await?;
    Ok(())
}
